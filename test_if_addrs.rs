use if_addrs::get_if_addrs;
fn main() {
    let ifaces = get_if_addrs().unwrap();
    for iface in ifaces {
        println!("{:?}", iface);
    }
}
