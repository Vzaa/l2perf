l2perf
------

l2perf is a networking tool for Linux that aims to be similar to iperf UDP mode
but for layer 2 (data link) networking. It utilizes the pnet library for data
link layer communication to generate and receive traffic.

### Use Cases

Although it doesn't sound very useful, it may come in handy if you just need to
generate some random traffic over a network to create a test case you need.
As long as there is a valid MAC destination address on the receiving end it
shouldn't require any prior connection or IP layer setup just to generate
traffic. There may be valid cases without an existing destination MAC address
but for most cases packets will likely get dropped on the other end of the
link.

If you run the tool also on the receiving end, it can be used to analyze
achievable bandwidth and drop rates in a similar way to iperf UDP.

### Usage

The tool allows you to configure the network interface, payload size,
ethertype, test duration and the target bandwidth. See `--help` for additional
info.

Note that the process will likely need elevated permissions. On Linux, it will specifically need the CAP_NET_RAW capability, which needs to be added once per build. As an example:

```
cargo build && sudo setcap 'cap_net_raw=ep' target/debug/l2perf
```

Running the program as root also works, but then it can also do things like install kernel modules or wipe your root filesystem. With just the `cap_net_raw` capability, the potential for damage is much more limited.

### Example Run (veth)

First, a little setup:

```
sudo ip link add type veth
: optionally, if you prefer to name the interfaces explictly,
: sudo ip link add dev veth0 type veth peer name veth1
sudo ip link set dev veth0 up
sudo ip link set dev veth1 up
```

For more details, see: https://baturin.org/docs/iproute2/#ip-link-add-veth

Then set up a receiver:

```
./target/debug/l2perf -i veth1 -r
```

and start the transmitter:

```
./target/debug/l2perf -i veth0 -b 10000 $(cat /sys/class/net/veth1/address)
```

Cleaning up:

```
sudo ip link del veth0
```

### Example Run (remote peer)

An example test output from a test with a laptop using Intel 802.11ac WiFi card
to a receiving Raspberry Pi 4 connected to a WiFi router over Ethernet.

#### Tx Side
```
$ ./l2perf  -i wlan0 -t 10 -b 350 dc:a6:32:01:01:01
Sec: 0.00-1.01, Sent: 27938 pkts, Rate: 332.59 Mbps
Sec: 1.01-2.01, Sent: 30537 pkts, Rate: 366.35 Mbps
Sec: 2.01-3.01, Sent: 28096 pkts, Rate: 337.15 Mbps
Sec: 3.01-4.01, Sent: 29490 pkts, Rate: 353.72 Mbps
Sec: 4.01-5.01, Sent: 29682 pkts, Rate: 356.18 Mbps
Sec: 5.01-6.01, Sent: 29501 pkts, Rate: 354.00 Mbps
Sec: 6.01-7.01, Sent: 27806 pkts, Rate: 333.18 Mbps
Sec: 7.01-8.01, Sent: 29324 pkts, Rate: 351.82 Mbps
Sec: 8.01-9.01, Sent: 29234 pkts, Rate: 350.67 Mbps
Summary:
Sec: 0.00-10.00, Sent: 291534 pkts, Rate: 349.82 Mbps
```

#### Rx Side
```
$ ./l2perf  -i eth0 -r
Accepting Ether Type 7380...

New incoming traffic:
Sec: 0.00-1.01, Recv: 27938/27938 pkts, Dropped: 0.00%, Rate: 332.70 Mbps
Sec: 1.01-2.01, Recv: 30247/30530 pkts, Dropped: 0.93%, Rate: 362.96 Mbps
Sec: 2.01-3.01, Recv: 27241/28075 pkts, Dropped: 2.97%, Rate: 326.89 Mbps
Sec: 3.01-4.01, Recv: 29413/29494 pkts, Dropped: 0.27%, Rate: 352.95 Mbps
Sec: 4.01-5.01, Recv: 29599/29663 pkts, Dropped: 0.22%, Rate: 355.12 Mbps
Sec: 5.01-6.01, Recv: 29558/29558 pkts, Dropped: 0.00%, Rate: 354.69 Mbps
Sec: 6.01-7.01, Recv: 27791/27792 pkts, Dropped: 0.00%, Rate: 333.19 Mbps
Sec: 7.01-8.01, Recv: 29303/29303 pkts, Dropped: 0.00%, Rate: 351.63 Mbps
Sec: 8.01-9.01, Recv: 29231/29231 pkts, Dropped: 0.00%, Rate: 350.68 Mbps
Summary:
Sec: 0.00-10.00, Recv: 290266/291534 pkts, Dropped: 0.43%, Rate: 348.31 Mbps
