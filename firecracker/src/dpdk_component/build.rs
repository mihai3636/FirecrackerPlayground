// use std::path::Path;

fn main() {

    let _src = ["src/static-functions.c"];
    println!("cargo:rerun-if-changed=build.rs");

    // let path1 = Path::new("/usr/local/include");
    // let path2 = Path::new("/usr/include/x86_64-linux-gnu");
    // let path3 = Path::new("/usr/include/linux");
    // let path4 = Path::new("/usr/include/asm-generic");

    let mut builder = cc::Build::new();
    let build = builder
        .file("src/static-functions.c")
        .flag("-Wno-unused-parameter")
        .flag("-lrte_ring")
        .flag("-lrte_mempool")
        .flag("-lrte_mbuf")
        .flag("-lrte_node")
        .flag("-lrte_graph")
        .flag("-lrte_bpf")
        .flag("-lrte_flow_classify")
        .flag("-lrte_pipeline")
        .flag("-lrte_table")
        .flag("-lrte_port")
        .flag("-lrte_fib")
        .flag("-lrte_ipsec")
        .flag("-lrte_vhost")
        .flag("-lrte_stack")
        .flag("-lrte_security")
        .flag("-lrte_sched")
        .flag("-lrte_reorder")
        .flag("-lrte_rib")
        .flag("-lrte_regexdev")
        .flag("-lrte_rawdev")
        .flag("-lrte_pdump")
        .flag("-lrte_power")
        .flag("-lrte_member")
        .flag("-lrte_lpm")
        .flag("-lrte_latencystats")
        .flag("-lrte_kni")
        .flag("-lrte_jobstats")
        .flag("-lrte_ip_frag")
        .flag("-lrte_gso")
        .flag("-lrte_gro")
        .flag("-lrte_eventdev")
        .flag("-lrte_efd")
        .flag("-lrte_distributor")
        .flag("-lrte_cryptodev")
        .flag("-lrte_compressdev")
        .flag("-lrte_cfgfile")
        .flag("-lrte_bitratestats")
        .flag("-lrte_bbdev")
        .flag("-lrte_acl")
        .flag("-lrte_timer")
        .flag("-lrte_hash")
        .flag("-lrte_metrics")
        .flag("-lrte_cmdline")
        .flag("-lrte_pci")
        .flag("-lrte_ethdev")
        .flag("-lrte_meter")
        .flag("-lrte_net")
        .flag("-lrte_rcu")
        .flag("-lrte_eal")
        .flag("-lrte_telemtry")
        .flag("-lrte_kvargs");
        // .include(path1)
        // .include(path2)
        // .include(path3)
        // .include(path4);
        // .flag("-I/usr/include/asm-generic")
        // .flag("-I/usr/include/linux")
        // .flag("-I/usr/local/include")
        // .flag("-I/usr/include/x86_64-linux-gnu");

    build.compile("foo");
}
