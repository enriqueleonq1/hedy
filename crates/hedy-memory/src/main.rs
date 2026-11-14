//! Standalone memory server, for trying it by hand. The app serves the same
//! thing as `hedy-app memory-mcp …`.

fn main() -> anyhow::Result<()> {
    hedy_memory::serve(hedy_memory::Options::parse(std::env::args().skip(1))?)
}
