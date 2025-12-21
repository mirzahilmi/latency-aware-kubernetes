use std::env;

use dioxus::prelude::*;

const FAVICON: Asset = asset!("/assets/favicon.ico");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let pod_name = env::var("POD_NAME").unwrap_or("N/A".to_string());
    let pod_ns = env::var("POD_NAMESPACE").unwrap_or("N/A".to_string());
    let pod_ip = env::var("POD_IP").unwrap_or("N/A".to_string());
    let pod_mem_limit = env::var("POD_MEM_LIMIT").unwrap_or("N/A".to_string());
    let pod_cpu_limit = env::var("POD_CPU_LIMIT").unwrap_or("N/A".to_string());
    let node_name = env::var("NODE_NAME").unwrap_or("N/A".to_string());
    let node_ip = env::var("NODE_IP").unwrap_or("N/A".to_string());

    let metas = vec![
        ("Node Name".to_string(), node_name),
        ("Node IP".to_string(), node_ip),
        ("Pod Name".to_string(), pod_name),
        ("Pod Namespace".to_string(), pod_ns),
        ("Pod IP".to_string(), pod_ip),
        ("Pod Memory Limit".to_string(), pod_mem_limit),
        ("Pod CPU Limit".to_string(), pod_cpu_limit),
    ];

    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Stylesheet { href: TAILWIND_CSS }
        div { class: "bg-gray-900 min-h-screen flex flex-col items-center justify-center",
            h1 { class: "text-center text-6xl font-bold text-white mb-8", "Hellopod 🐳" }
            div { class: "text-gray-300 text-lg space-y-2 max-w-2xl text-left mb-16",
                for (meta , meta_data) in metas {
                    p { key: "{meta}", class: "",
                        span { class: "font-semibold inline-block w-3xs", "{meta}: " }
                        "{meta_data}"
                    }
                }
            }
            p { class: "text-white text-md",
                "Built with ❤️‍🩹 by "
                a {
                    class: "font-medium text-blue-600 dark:text-blue-500 hover:underline",
                    href: "https://github.com/mirzahilmi",
                    target: "_blank",
                    "github.com/mirzahilmi"
                }
                " w/ Dioxus"
            }
        }
    }
}
