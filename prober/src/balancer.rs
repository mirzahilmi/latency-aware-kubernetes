use kube::ResourceExt;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use futures::{TryStreamExt, lock::Mutex};
use k8s_openapi::api::discovery::v1::EndpointSlice;
use kube::{
    Api, Client,
    runtime::{self, WatchStreamExt, watcher::Config},
};
use nftables::{
    expr::{Expression, Verdict},
    helper,
    schema::{NfListObject, NfObject},
    stmt::Statement,
};
use regex::Regex;
use tracing::info;

#[derive(Debug, Clone, Default)]
pub struct NftablesService {
    id: String,
    vmap_handle: u32,
    endpoints: HashSet<NftablesEndpoint>,
}

#[derive(Debug, Clone, Default, Hash, PartialEq, Eq)]
pub struct NftablesEndpoint {
    id: String,
    hostname: String,
}

pub async fn watch_service_updates(
    client: Client,
    nftables_chains_lookup: Arc<Mutex<HashMap<String, NftablesService>>>,
) -> anyhow::Result<()> {
    // exclude kube-system
    let endpoints: Api<EndpointSlice> = Api::all(client);
    let regex_ipv4 = Arc::new(Regex::new(r"((25[0-5]|(2[0-4]|1\d|[1-9]|)\d)\.?\b){4}")?);
    info!("Watching for EndpointSlice updates");
    runtime::watcher(endpoints, Config::default())
        .applied_objects()
        .default_backoff()
        .try_for_each(|endpointslice| {
            let regex_ipv4 = regex_ipv4.clone();
            let mut ipv4_lookup = HashMap::new();
            let lookup = nftables_chains_lookup.clone();
            endpointslice.endpoints.iter().for_each(|endpoint| {
                let Some(backend) = endpoint.addresses.first() else {
                    return;
                };
                let Some(hostname) = &endpoint.node_name else {
                    return;
                };
                ipv4_lookup.insert(backend.clone(), hostname.clone());
            });
            async move {
                info!("EndpointSlice changes occured: {endpointslice:?}");

                // acquire lock early to block lb update
                let lookup = lookup.lock().await;

                let Some(service) = endpointslice.labels().get("kubernetes.io/service-name") else {
                    return Ok(());
                };

                let Ok(regex_service_chain) =
                    Regex::new(&format!(r"(service-{service}-\S+\/\S+\/(?:tcp|udp)\/)"))
                else {
                    return Ok(());
                };

                let Ok(chains) =
                    helper::get_current_ruleset_raw(helper::DEFAULT_NFT, ["list", "chains", "ip"])
                else {
                    return Ok(());
                };
                let Some(service_chain) = regex_service_chain.find(&chains) else {
                    return Ok(());
                };
                let service_chain = service_chain.as_str().to_string();

                let Ok(nftables_chain) = helper::get_current_ruleset_with_args(
                    helper::DEFAULT_NFT,
                    ["list", "chain", "ip"],
                ) else {
                    return Ok(());
                };

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
                        let Expression::List(expressions) = &vmap.data else {
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
                let Some(service_chain_handle) = service_chain_handle else {
                    return Ok(());
                };

                let mut nftables_sevice = match lookup.get(service) {
                    Some(nftable_service) => nftable_service.clone(),
                    None => NftablesService::default(),
                };
                nftables_sevice.id = service_chain;
                nftables_sevice.vmap_handle = service_chain_handle;

                endpoint_chains.iter().for_each(|chain| {
                    let Some(ipv4) = regex_ipv4.find(chain) else {
                        return;
                    };
                    let ipv4 = ipv4.as_str().to_owned();
                    let Some(hostname) = ipv4_lookup.get(&ipv4) else {
                        return;
                    };
                    nftables_sevice.endpoints.insert(NftablesEndpoint {
                        id: chain.clone(),
                        hostname: hostname.clone(),
                    });
                });

                Ok(())
            }
        })
        .await?;

    Ok(())
}

pub async fn reconcile(
    cost_caches: Arc<Mutex<HashMap<String, f64>>>,
    nftables_chains_lookup: Arc<Mutex<HashMap<String, NftablesService>>>,
) -> anyhow::Result<()> {
    let nftables = nftables_chains_lookup.lock().await;
    info!("NftablesUpdate: {:?}", nftables);
    Ok(())
}
