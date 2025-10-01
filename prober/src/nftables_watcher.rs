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
            return Ok(());
        }

        // exclude kube-system
        let endpoints: Api<Endpoints> = Api::all(self.kube_client.clone());
        let regex_ipv4 = Arc::new(Regex::new(r"((25[0-5]|(2[0-4]|1\d|[1-9]|)\d)\.?\b){4}")?);

        // see https://kube.rs/controllers/optimization/#reducing-number-of-watched-objects
        let exclude_system_namespaces = [
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
        .join(",");

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
                let Some(EndpointSubset {
                    addresses: Some(addresses),
                    ..
                }) = endpointslice
                    .subsets
                    .as_ref()
                    .and_then(|subsets| subsets.first())
                else {
                    return Ok(());
                };
                let mut hostname_by_ipv4 = HashMap::new();
                addresses.iter().for_each(|address| {
                    let Some(hostname) = &address.node_name else {
                        return;
                    };
                    hostname_by_ipv4.insert(address.ip.clone(), hostname.clone());
                });
                debug!("nftables_watcher: host by ipv4 address: {hostname_by_ipv4:?}");

                let mut nftables_chain_by_service = nftables_chain_by_service.lock().await;

                let Some(service) = endpointslice.name() else {
                    error!("nftables_watcher: could not endpoints service name");
                    return Ok(());
                };
                let service = service.to_string();
                if service == "kubernetes" {
                    return Ok(());
                }
                info!("nftables_watcher: changes occured on service {service}");

                let pattern = format!(r"(service-[A-Z0-9]{{8}}-\S+\/{service}\/(?:tcp|udp)\/)");
                let regex_service_chain = match Regex::new(&pattern) {
                    Ok(regex_service_chain) => regex_service_chain,
                    Err(e) => {
                        error!("Failed to parse service chain regex: {e}");
                        return Ok(());
                    }
                };

                let chains = match helper::get_current_ruleset_raw(
                    helper::DEFAULT_NFT,
                    ["list", "chains", "ip"],
                ) {
                    Ok(chains) => chains,
                    Err(e) => {
                        error!("Failed to get current nftables rulesets: {e:?}");
                        return Ok(());
                    }
                };

                let service_chain = match regex_service_chain.find(&chains) {
                    Some(service_chain) => service_chain,
                    None => {
                        warn!("nftables_watcher: cannot find chain id for service {service}");
                        return Ok(());
                    }
                };
                let service_chain = service_chain.as_str().to_string();

                tokio::time::sleep(Duration::from_secs(5)).await;
                let nftables_chain = match helper::get_current_ruleset_with_args(
                    helper::DEFAULT_NFT,
                    ["list", "chain", "ip", "kube-proxy", service_chain.as_str()],
                ) {
                    Ok(nftables_chain) => nftables_chain,
                    Err(e) => {
                        error!("nftables_watcher: failed to get current nftables rulesets: {e:?}");
                        return Ok(());
                    }
                };
                debug!("nftables_watcher: received nftables_chain: {nftables_chain:?}");

                let mut service_chain_handle: Option<u32> = None;
                let mut endpoint_chains: Vec<String> = vec![];
                // big ass loop, there must be a better way
                nftables_chain.objects.into_owned().iter().for_each(|obj| {
                    let NfObject::ListObject(NfListObject::Rule(rule)) = obj else {
                        return;
                    };
                    rule.expr.iter().for_each(|statement| {
                        let Statement::VerdictMap(vmap) = statement else {
                            return;
                        };
                        service_chain_handle = rule.handle;
                        let Expression::Named(NamedExpression::Set(set)) = &vmap.data else {
                            return;
                        };
                        set.iter().for_each(|item| {
                            let SetItem::Element(Expression::List(expressions)) = item else {
                                return;
                            };
                            expressions.iter().for_each(|expr| {
                                let Expression::Verdict(Verdict::Goto(goto)) = expr else {
                                    return;
                                };
                                endpoint_chains.push(goto.clone().target.into_owned());
                            });
                        });
                    });
                });
                let Some(service_chain_handle) = service_chain_handle else {
                    return Ok(());
                };

                let mut nftables_sevice = match nftables_chain_by_service.get(&service) {
                    Some(nftable_service) => nftable_service.clone(),
                    None => NftablesService::default(),
                };
                nftables_sevice.applied = false;
                nftables_sevice.id = service_chain;
                nftables_sevice.vmap_handle = service_chain_handle;

                let mut backends: HashMap<String, HashSet<String>> = HashMap::new();
                endpoint_chains.iter().for_each(|chain| {
                    let Some(ipv4) = regex_ipv4.find(chain) else {
                        return;
                    };
                    let ipv4 = ipv4.as_str().to_owned();
                    let Some(hostname) = hostname_by_ipv4.get(&ipv4) else {
                        return;
                    };
                    match backends.get_mut(hostname) {
                        Some(endpoints) => {
                            endpoints.insert(chain.to_owned());
                        }
                        None => {
                            backends.insert(hostname.to_owned(), HashSet::from([chain.to_owned()]));
                        }
                    }
                });
                nftables_sevice.endpoints_by_host = backends;

                debug!("nftables_watcher: inserting service chain: {nftables_sevice:?}");
                nftables_chain_by_service.insert(service, nftables_sevice);

                Ok(())
            }
        })
        .await?;

        Ok(())
    }

    async fn init(&mut self) -> anyhow::Result<()> {
        // exclude kube-system
        let endpoints: Api<Endpoints> = Api::all(self.kube_client.clone());
        let regex_ipv4 = Arc::new(Regex::new(r"((25[0-5]|(2[0-4]|1\d|[1-9]|)\d)\.?\b){4}")?);

        // see https://kube.rs/controllers/optimization/#reducing-number-of-watched-objects
        let exclude_system_namespaces = [
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
        .join(",");

        let endpoints = endpoints
            .list(&ListParams::default().fields(exclude_system_namespaces.as_str()))
            .await?;

        // acquire lock early to block lb update
        let mut nftables_chain_by_service = self.nftables_chain_by_service.lock().await;

        for endpointslice in endpoints {
            let Some(EndpointSubset {
                addresses: Some(addresses),
                ..
            }) = endpointslice
                .subsets
                .as_ref()
                .and_then(|subsets| subsets.first())
            else {
                error!("nftables_watcher: could not retrieve endpoint addresses");
                continue;
            };
            let mut hostname_by_ipv4 = HashMap::new();
            addresses.iter().for_each(|address| {
                let Some(hostname) = &address.node_name else {
                    return;
                };
                hostname_by_ipv4.insert(address.ip.clone(), hostname.clone());
            });
            debug!("nftables_watcher: host by ipv4 address: {hostname_by_ipv4:?}");

            let Some(service) = endpointslice.name() else {
                error!("nftables_watcher: could not endpoints service name");
                continue;
            };
            let service = service.to_string();
            if service == "kubernetes" {
                continue;
            }
            info!("nftables_watcher: changes occured on service {service}");

            let pattern = format!(r"(service-[A-Z0-9]{{8}}-\S+\/{service}\/(?:tcp|udp)\/)");
            let regex_service_chain = match Regex::new(&pattern) {
                Ok(regex_service_chain) => regex_service_chain,
                Err(e) => {
                    error!("Failed to parse service chain regex: {e}");
                    continue;
                }
            };

            let chains = match helper::get_current_ruleset_raw(
                helper::DEFAULT_NFT,
                ["list", "chains", "ip"],
            ) {
                Ok(chains) => chains,
                Err(e) => {
                    error!("Failed to get current nftables rulesets: {e:?}");
                    continue;
                }
            };

            let service_chain = match regex_service_chain.find(&chains) {
                Some(service_chain) => service_chain,
                None => {
                    warn!("nftables_watcher: cannot find chain id for service {service}");
                    continue;
                }
            };
            let service_chain = service_chain.as_str().to_string();

            tokio::time::sleep(Duration::from_secs(5)).await;
            let nftables_chain = match helper::get_current_ruleset_with_args(
                helper::DEFAULT_NFT,
                ["list", "chain", "ip", "kube-proxy", service_chain.as_str()],
            ) {
                Ok(nftables_chain) => nftables_chain,
                Err(e) => {
                    error!("nftables_watcher: failed to get current nftables rulesets: {e:?}");
                    continue;
                }
            };
            debug!("nftables_watcher: received nftables_chain: {nftables_chain:?}");

            let mut service_chain_handle: Option<u32> = None;
            let mut endpoint_chains: Vec<String> = vec![];
            // big ass loop, there must be a better way
            nftables_chain.objects.into_owned().iter().for_each(|obj| {
                let NfObject::ListObject(NfListObject::Rule(rule)) = obj else {
                    return;
                };
                rule.expr.iter().for_each(|statement| {
                    let Statement::VerdictMap(vmap) = statement else {
                        return;
                    };
                    service_chain_handle = rule.handle;
                    let Expression::Named(NamedExpression::Set(set)) = &vmap.data else {
                        return;
                    };
                    set.iter().for_each(|item| {
                        let SetItem::Element(Expression::List(expressions)) = item else {
                            return;
                        };
                        expressions.iter().for_each(|expr| {
                            let Expression::Verdict(Verdict::Goto(goto)) = expr else {
                                return;
                            };
                            endpoint_chains.push(goto.clone().target.into_owned());
                        });
                    });
                });
            });
            let Some(service_chain_handle) = service_chain_handle else {
                continue;
            };

            let mut nftables_sevice = match nftables_chain_by_service.get(&service) {
                Some(nftable_service) => nftable_service.clone(),
                None => NftablesService::default(),
            };
            nftables_sevice.applied = false;
            nftables_sevice.id = service_chain;
            nftables_sevice.vmap_handle = service_chain_handle;

            let mut backends: HashMap<String, HashSet<String>> = HashMap::new();
            endpoint_chains.iter().for_each(|chain| {
                let Some(ipv4) = regex_ipv4.find(chain) else {
                    return;
                };
                let ipv4 = ipv4.as_str().to_owned();
                let Some(hostname) = hostname_by_ipv4.get(&ipv4) else {
                    return;
                };
                match backends.get_mut(hostname) {
                    Some(endpoints) => {
                        endpoints.insert(chain.to_owned());
                    }
                    None => {
                        backends.insert(hostname.to_owned(), HashSet::from([chain.to_owned()]));
                    }
                }
            });
            nftables_sevice.endpoints_by_host = backends;

            debug!("nftables_watcher: inserting service chain: {nftables_sevice:?}");
            nftables_chain_by_service.insert(service, nftables_sevice);
        }

        Ok(())
    }
}
