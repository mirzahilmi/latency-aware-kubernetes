use kube::runtime;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use futures::{TryStreamExt, lock::Mutex};
use k8s_openapi::api::core::v1::{EndpointSubset, Endpoints};
use kube::{
    Api, Client,
    api::ListParams,
    runtime::{WatchStreamExt, reflector::Lookup, watcher::Config},
};
use nftables::{
    expr::{Expression, NamedExpression, SetItem, Verdict},
    helper,
    schema::{NfListObject, NfObject},
    stmt::Statement,
};
use regex::Regex;
use tracing::{debug, error, info, warn};

type Hostname = String;
type EndpointChainId = String;

#[derive(Debug, Clone, Default)]
pub struct NftablesService {
    pub id: String,
    pub vmap_handle: u32,
    pub applied: bool,
    pub endpoints_by_host: HashMap<Hostname, HashSet<EndpointChainId>>,
}

pub struct NftablesWatcher {
    pub shutdown_sig: tokio::sync::broadcast::Receiver<()>,
    pub kube_client: Client,
    pub nftables_chain_by_service: Arc<Mutex<HashMap<String, NftablesService>>>,
}

impl NftablesWatcher {
    pub async fn run(&mut self) -> anyhow::Result<()> {
        info!("nftables_watcher: initialize background process");

        if let Err(e) = self.init().await {
            error!("nftables_watcher: failed to init nftables service chains: {e}");
            return Err(e);
        }

        // exclude kube-system
        let endpoints: Api<Endpoints> = Api::all(self.kube_client.clone());
        let regex_ipv4 = Arc::new(Regex::new(r"\b(?:\d{1,3}\.){3}\d{1,3}\b")?);

        let exclude_system_namespaces = Self::get_excluded_namespaces();

        runtime::watcher(
            endpoints,
            Config::default().fields(&exclude_system_namespaces),
        )
        .applied_objects()
        .default_backoff()
        .try_for_each(|endpointslice| {
            let regex_ipv4 = regex_ipv4.clone();
            let nftables_chain_by_service = self.nftables_chain_by_service.clone();
            async move {
                if let Err(e) =
                    Self::process_endpoint(endpointslice, regex_ipv4, nftables_chain_by_service)
                        .await
                {
                    error!("nftables_watcher: failed to process endpoint: {e}");
                }
                Ok(())
            }
        })
        .await?;

        Ok(())
    }

    async fn process_endpoint(
        endpointslice: Endpoints,
        regex_ipv4: Arc<Regex>,
        nftables_chain_by_service: Arc<Mutex<HashMap<String, NftablesService>>>,
    ) -> anyhow::Result<()> {
        let Some(EndpointSubset {
            addresses: Some(addresses),
            ..
        }) = endpointslice
            .subsets
            .as_ref()
            .and_then(|subsets| subsets.first())
        else {
            debug!("nftables_watcher: no addresses found in endpoint");
            return Ok(());
        };

        let mut hostname_by_ipv4 = HashMap::new();
        addresses.iter().for_each(|address| {
            if let Some(hostname) = &address.node_name {
                hostname_by_ipv4.insert(address.ip.clone(), hostname.clone());
            }
        });
        debug!("nftables_watcher: host by ipv4 address: {hostname_by_ipv4:?}");

        let Some(service) = endpointslice.name() else {
            warn!("nftables_watcher: could not get endpoints service name");
            return Ok(());
        };
        let service = service.to_string();

        if service == "kubernetes" {
            return Ok(());
        }

        info!("nftables_watcher: processing service {service}");

        let nftables_service =
            Self::fetch_service_chain_info(&service, &hostname_by_ipv4, &regex_ipv4).await?;

        let mut nftables_chain_by_service = nftables_chain_by_service.lock().await;
        debug!("nftables_watcher: inserting service chain: {nftables_service:?}");
        nftables_chain_by_service.insert(service, nftables_service);

        Ok(())
    }

    async fn fetch_service_chain_info(
        service: &str,
        hostname_by_ipv4: &HashMap<String, String>,
        regex_ipv4: &Regex,
    ) -> anyhow::Result<NftablesService> {
        // Allow both uppercase and lowercase in chain IDs
        let pattern = format!(r"(service-[A-Za-z0-9]{{8}}-\S+/{service}/(?:tcp|udp)/)");
        let regex_service_chain = Regex::new(&pattern)
            .map_err(|e| anyhow::anyhow!("Failed to parse service chain regex: {e}"))?;

        let chains = helper::get_current_ruleset_raw(helper::DEFAULT_NFT, ["list", "chains", "ip"])
            .map_err(|e| anyhow::anyhow!("Failed to get nftables chains: {e:?}"))?;

        let service_chain = regex_service_chain
            .find(&chains)
            .ok_or_else(|| anyhow::anyhow!("Cannot find chain id for service {service}"))?
            .as_str()
            .to_string();

        info!("nftables_watcher: found service chain: {service_chain}");

        // Wait for kube-proxy to finish updating
        tokio::time::sleep(Duration::from_secs(2)).await;

        let nftables_chain = helper::get_current_ruleset_with_args(
            helper::DEFAULT_NFT,
            ["list", "chain", "ip", "kube-proxy", service_chain.as_str()],
        )
        .map_err(|e| anyhow::anyhow!("Failed to get chain details for {service_chain}: {e:?}"))?;

        debug!("nftables_watcher: received nftables_chain: {nftables_chain:?}");

        let (service_chain_handle, endpoint_chains) = Self::parse_nftables_chain(&nftables_chain)?;

        info!(
            "nftables_watcher: found {} endpoint chains for service {service}",
            endpoint_chains.len()
        );

        let endpoints_by_host =
            Self::map_endpoints_to_hosts(&endpoint_chains, hostname_by_ipv4, regex_ipv4);

        Ok(NftablesService {
            id: service_chain,
            vmap_handle: service_chain_handle,
            applied: false,
            endpoints_by_host,
        })
    }

    fn parse_nftables_chain(
        nftables_chain: &nftables::schema::Nftables,
    ) -> anyhow::Result<(u32, Vec<String>)> {
        let mut service_chain_handle: Option<u32> = None;
        let mut endpoint_chains: Vec<String> = vec![];

        for obj in nftables_chain.objects.iter() {
            let NfObject::ListObject(NfListObject::Rule(rule)) = obj else {
                continue;
            };

            for statement in rule.expr.iter() {
                let Statement::VerdictMap(vmap) = statement else {
                    continue;
                };

                service_chain_handle = rule.handle;

                let Expression::Named(NamedExpression::Set(set)) = &vmap.data else {
                    continue;
                };

                for item in set.iter() {
                    let SetItem::Element(Expression::List(expressions)) = item else {
                        continue;
                    };

                    for expr in expressions.iter() {
                        if let Expression::Verdict(Verdict::Goto(goto)) = expr {
                            endpoint_chains.push(goto.clone().target.into_owned());
                        }
                    }
                }
            }
        }

        let handle = service_chain_handle
            .ok_or_else(|| anyhow::anyhow!("Could not find service chain handle"))?;

        Ok((handle, endpoint_chains))
    }

    fn map_endpoints_to_hosts(
        endpoint_chains: &[String],
        hostname_by_ipv4: &HashMap<String, String>,
        regex_ipv4: &Regex,
    ) -> HashMap<String, HashSet<String>> {
        let mut backends: HashMap<String, HashSet<String>> = HashMap::new();

        for chain in endpoint_chains {
            let Some(ipv4_match) = regex_ipv4.find(chain) else {
                warn!("nftables_watcher: no IPv4 found in chain: {chain}");
                continue;
            };

            let ipv4 = ipv4_match.as_str();
            let Some(hostname) = hostname_by_ipv4.get(ipv4) else {
                warn!("nftables_watcher: no hostname mapping for IP {ipv4}");
                continue;
            };

            backends
                .entry(hostname.clone())
                .or_default()
                .insert(chain.clone());
        }

        backends
    }

    async fn init(&mut self) -> anyhow::Result<()> {
        info!("nftables_watcher: starting initialization");

        let endpoints: Api<Endpoints> = Api::all(self.kube_client.clone());
        let regex_ipv4 = Arc::new(Regex::new(r"\b(?:\d{1,3}\.){3}\d{1,3}\b")?);
        let exclude_system_namespaces = Self::get_excluded_namespaces();

        let endpoints = endpoints
            .list(&ListParams::default().fields(exclude_system_namespaces.as_str()))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to list endpoints: {e}"))?;

        info!(
            "nftables_watcher: found {} endpoints to process",
            endpoints.items.len()
        );

        // Acquire lock early to block lb update
        let mut nftables_chain_by_service = self.nftables_chain_by_service.lock().await;
        let mut success_count = 0;
        let mut failure_count = 0;

        for endpointslice in endpoints {
            let Some(EndpointSubset {
                addresses: Some(addresses),
                ..
            }) = endpointslice
                .subsets
                .as_ref()
                .and_then(|subsets| subsets.first())
            else {
                debug!("nftables_watcher: skipping endpoint with no addresses");
                continue;
            };

            let mut hostname_by_ipv4 = HashMap::new();
            addresses.iter().for_each(|address| {
                if let Some(hostname) = &address.node_name {
                    hostname_by_ipv4.insert(address.ip.clone(), hostname.clone());
                }
            });

            let Some(service) = endpointslice.name() else {
                warn!("nftables_watcher: could not get endpoints service name");
                continue;
            };
            let service = service.to_string();

            if service == "kubernetes" {
                continue;
            }

            match Self::fetch_service_chain_info(&service, &hostname_by_ipv4, &regex_ipv4).await {
                Ok(nftables_service) => {
                    info!("nftables_watcher: successfully initialized service {service}");
                    nftables_chain_by_service.insert(service, nftables_service);
                    success_count += 1;
                }
                Err(e) => {
                    error!("nftables_watcher: failed to initialize service {service}: {e}");
                    failure_count += 1;
                }
            }
        }

        info!(
            "nftables_watcher: initialization complete - {} succeeded, {} failed",
            success_count, failure_count
        );

        if success_count == 0 && failure_count > 0 {
            return Err(anyhow::anyhow!(
                "Failed to initialize any services ({} failures)",
                failure_count
            ));
        }

        Ok(())
    }

    fn get_excluded_namespaces() -> String {
        [
            "cert-manager",
            "flux2",
            "linkerd",
            "linkerd-jaeger",
            "linkerd-smi",
            "linkerd-viz",
            "gatekeeper-system",
            "kube-node-lease",
            "kube-public",
            "kube-system",
        ]
        .into_iter()
        .map(|ns| format!("metadata.namespace!={ns}"))
        .collect::<Vec<_>>()
        .join(",")
    }
}
