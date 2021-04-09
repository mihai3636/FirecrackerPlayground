fn main() {
    // Tell cargo to tell rustc to link the system bzip2
    // shared library.
    // println!("cargo:rustc-link-lib=rte_ring");
    // println!("cargo:rustc-link-lib=rte_mempool");

    // Tell cargo to invalidate the built crate whenever the wrapper changes
    // println!("cargo:rerun-if-changed=wrapper.h");

    let _src = ["src/static-functions.c"];

    let mut builder = cc::Build::new();
    let build = builder
        .file("src/static-functions.c")
        .flag("-Wno-unused-parameter")
        .flag("-lrte_ring")
        .flag("-lrte_mempool")
        .flag("-lrte_mbuf");
    build.compile("foo");
}
