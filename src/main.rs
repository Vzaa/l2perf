use std::io::ErrorKind;
use std::num::ParseIntError;
use std::time::{Duration, Instant};
use std::{collections::HashMap, thread};

use rand::prelude::*;

use serde::{Deserialize, Serialize};

use structopt::StructOpt;

use pnet::datalink::Channel::Ethernet;
use pnet::datalink::{self, ChannelType, Config};
use pnet::packet::ethernet::{EtherType, MutableEthernetPacket};
use pnet::packet::Packet;
use pnet::util::MacAddr;

// Ethernet Header Size: SRC(6) + DST(6) + EtherType(2) = 14
const ETH_HEADER_SIZE: usize = 14;

#[derive(StructOpt, Debug)]
#[structopt(name = "l2perf")]
struct Opt {
    #[structopt(short, long, default_value = "1.0", help = "Bandwidth in Mbits/s")]
    bandwidth: f32,
    #[structopt(short, long, default_value = "10", help = "Duration in seconds")]
    tsecs: u64,
    #[structopt(short, long, default_value = "7380", parse(try_from_str = parse_hex), help = "Ethertype in hex")]
    ethertype: u16,
    #[structopt(short, long, default_value = "1500", help = "Payload size")]
    psize: usize,
    #[structopt(short, long, default_value = "eth0", help = "Network interface")]
    ifname: String,
    #[structopt(
        name = "DEST",
        required_unless("rx"),
        help = "Destination MAC addr for TX mode"
    )]
    dest: Option<MacAddr>,
    #[structopt(short, long, help = "RX mode")]
    rx: bool,
}

fn parse_hex(src: &str) -> Result<u16, ParseIntError> {
    u16::from_str_radix(src, 16)
}

#[derive(Debug, Serialize, Deserialize)]
struct Id {
    id: u32,
    cnt: u64,
    last: bool,
}

impl Id {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            cnt: 0,
            last: false,
        }
    }

    fn next(self) -> Self {
        Self {
            id: self.id,
            cnt: self.cnt + 1,
            last: false,
        }
    }
}

#[derive(Debug)]
struct Tracker {
    begin: Instant,
    total_bytes: u64,
    last_ptr: usize,
    last_rep: Instant,
    pkts: Vec<(Instant, u64, u64)>, // (timestamp, id, size)
}

impl Tracker {
    pub fn new() -> Self {
        Self {
            begin: Instant::now(),
            last_rep: Instant::now(),
            last_ptr: 0,
            total_bytes: 0,
            pkts: vec![],
        }
    }

    fn insert(&mut self, id: &Id, len: u64) {
        if let Some(last) = self.pkts.last() {
            if last.1 > id.cnt {
                eprintln!("Out of order recv!");
            }
        }
        self.total_bytes += len;
        self.pkts.push((Instant::now(), id.cnt, len));
    }

    fn report_rx(&mut self) {
        let since_last = self.last_rep.elapsed().as_secs_f32();
        if since_last >= 1.0 {
            let since_begin = self.begin.elapsed().as_secs_f32();
            let chunk = &self.pkts[self.last_ptr..];
            let bytes: u64 = chunk.iter().map(|p| p.2).sum();

            let cur_rate = ((8 * bytes) as f32) / since_last;

            let id_diff = chunk.last().unwrap().1 - chunk[0].1 + 1;
            let dropped = id_diff - chunk.len() as u64;
            let percent = (dropped as f32 / id_diff as f32) * 100.0;

            println!(
                "Sec: {:.2}-{:.2}, Recv: {}/{} pkts, Dropped: {:.2}%, Rate: {:.2} Mbps",
                since_begin - since_last,
                since_begin,
                chunk.len(),
                id_diff,
                percent,
                cur_rate / 1_000_000.0
            );

            self.last_rep = Instant::now();
            self.last_ptr = self.pkts.len() - 1;
        }
    }

    fn report_tx(&mut self) {
        let since_last = self.last_rep.elapsed().as_secs_f32();
        if since_last >= 1.0 {
            let since_begin = self.begin.elapsed().as_secs_f32();
            let chunk = &self.pkts[self.last_ptr..];
            let bytes: u64 = chunk.iter().map(|p| p.2).sum();

            let cur_rate = ((8 * bytes) as f32) / since_last;

            println!(
                "Sec: {:.2}-{:.2}, Sent: {} pkts, Rate: {:.2} Mbps",
                since_begin - since_last,
                since_begin,
                chunk.len(),
                cur_rate / 1_000_000.0
            );

            self.last_rep = Instant::now();
            self.last_ptr = self.pkts.len() - 1;
        }
    }

    fn report_rx_summary(&self) {
        let since_begin = self.begin.elapsed().as_secs_f32();
        let since_end = self.pkts.last().unwrap().0.elapsed().as_secs_f32();

        let id_diff = self.pkts.last().unwrap().1 - self.pkts[0].1 + 1;
        let dropped = id_diff - self.pkts.len() as u64;
        let percent = (dropped as f32 / id_diff as f32) * 100.0;

        let rate_tot = ((8 * self.total_bytes) as f32) / (since_begin - since_end);

        println!(
            "Summary:\nSec: 0.00-{:.2}, Recv: {}/{} pkts, Dropped: {:.2}%, Rate: {:.2} Mbps",
            since_begin - since_end,
            self.pkts.len(),
            id_diff,
            percent,
            rate_tot / 1_000_000.0
        );
    }

    fn report_tx_summary(&self) {
        let since_begin = self.begin.elapsed().as_secs_f32();

        let rate_tot = ((8 * self.total_bytes) as f32) / since_begin;

        println!(
            "Summary:\nSec: 0.00-{:.2}, Sent: {} pkts, Rate: {:.2} Mbps",
            since_begin,
            self.pkts.len(),
            rate_tot / 1_000_000.0
        );
    }
}

fn tx_traffic(tx: &mut Box<dyn datalink::DataLinkSender>, mac_addr_src: MacAddr, opts: Opt) {
    let mut dat = vec![0; opts.psize + ETH_HEADER_SIZE];
    let mut packet = MutableEthernetPacket::new(&mut dat).unwrap();
    packet.set_ethertype(EtherType::new(opts.ethertype));
    packet.set_source(mac_addr_src);
    packet.set_destination(opts.dest.unwrap());

    let mut rng = rand::thread_rng();

    let begin = Instant::now();
    let dur = Duration::from_secs(opts.tsecs);
    let resolution = Duration::from_millis(10);

    let mut id = Id::new(rng.gen());

    let mut tracker = Tracker::new();
    let mut buf = [0; 32];

    loop {
        let elapsed = begin.elapsed();

        let cur_rate = ((8 * tracker.total_bytes) as f32) / (elapsed.as_secs_f32());

        if cur_rate > opts.bandwidth * 1_000_000.0 {
            // TODO: Dynamic sleep time calculation?
            thread::sleep(resolution);
        } else {
            bincode::serialize_into(&mut buf[..], &id).unwrap();
            packet.set_payload(&buf);
            tx.send_to(packet.packet(), None).unwrap().unwrap();
            tracker.insert(&id, (opts.psize) as u64);
            id = id.next();
        }

        tracker.report_tx();

        if elapsed > dur {
            // Inform done
            id.last = true;
            bincode::serialize_into(&mut buf[..], &id).unwrap();
            packet.set_payload(&buf);
            tx.send_to(packet.packet(), None).unwrap().unwrap();
            break;
        }
    }

    tracker.report_tx_summary();
}

fn rx_traffic(rx: &mut Box<dyn datalink::DataLinkReceiver>, opts: Opt) {
    let mut trackers = HashMap::new();

    println!("Accepting Ether Type {:x}...", opts.ethertype);

    loop {
        match rx.next() {
            Ok(packet_raw) => {
                let len = packet_raw.len();

                let id: Id = bincode::deserialize(&packet_raw).unwrap();
                let tracker = trackers.entry(id.id).or_insert_with(Tracker::new);

                if tracker.total_bytes == 0 {
                    println!("\nNew incoming traffic:");
                }

                tracker.report_rx();

                if id.last {
                    tracker.report_rx_summary();
                    trackers.remove(&id.id);
                    continue;
                }
                tracker.insert(&id, len as u64);
            }
            Err(e) if matches!(e.kind(), ErrorKind::TimedOut) => {
                // Handle if the last packet was dropped
                for t in trackers.values() {
                    t.report_rx_summary();
                }
                trackers.clear();
            }
            Err(e) => {
                panic!("An error occurred while reading: {}", e);
            }
        }
    }
}

fn main() {
    let opt = Opt::from_args();

    let interface = datalink::interfaces()
        .into_iter()
        .find(|iface| iface.name == opt.ifname)
        .expect("Network interface not found");

    let mut config: Config = Default::default();

    if opt.rx {
        config.channel_type = ChannelType::Layer3(opt.ethertype);
        config.read_timeout = Some(Duration::from_secs(2));
    }

    let (mut tx, mut rx) = match datalink::channel(&interface, config) {
        Ok(Ethernet(tx, rx)) => (tx, rx),
        Ok(_) => panic!("Unhandled channel type"),
        Err(e) => panic!(
            "An error occurred when creating the datalink channel: {}",
            e
        ),
    };

    if opt.rx {
        rx_traffic(&mut rx, opt);
    } else {
        tx_traffic(&mut tx, interface.mac.unwrap(), opt);
    }
}
