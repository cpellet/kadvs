use async_std::{io, task};
use futures::{prelude::*, select};
use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::{record::Key, AddProviderOk, Kademlia, KademliaEvent, PeerRecord, PutRecordOk, QueryResult, Quorum, Record};
use libp2p::{development_transport, identity, mdns::{Mdns, MdnsConfig, MdnsEvent},swarm::{NetworkBehaviourEventProcess, SwarmEvent},NetworkBehaviour,PeerId,Swarm};
use std::error::Error;

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>>{
   env_logger::init();
   let local_key = identity::Keypair::generate_ed25519();
   let local_peer_id = PeerId::from(local_key.public());

    println!("Local peer id: {:?}", local_peer_id);

   let transport = development_transport(local_key).await?;

   #[derive(NetworkBehaviour)]
   #[behaviour(event_process = true)]
   struct MyBehaviour{
    kademlia: Kademlia<MemoryStore>,
    mdns: Mdns
   }

   impl NetworkBehaviourEventProcess<MdnsEvent> for MyBehaviour{
    fn inject_event(&mut self, event: MdnsEvent) {
        if let MdnsEvent::Discovered(list) = event{
            for (peer_id, multiaddr) in list{
                self.kademlia.add_address(&peer_id, multiaddr);
            }
        }
    }
   }

   impl NetworkBehaviourEventProcess<KademliaEvent> for MyBehaviour {
    fn inject_event(&mut self, event: KademliaEvent) {
        match event{
            KademliaEvent::OutboundQueryCompleted { result, ..} => match result{
                QueryResult::GetProviders(Ok(ok)) => {
                    for peer in ok.providers{
                        println!("Peer {:?} provides key {:?}", peer, std::str::from_utf8(ok.key.as_ref()).unwrap());
                    }
                }
                QueryResult::GetProviders(Err(err)) => {
                    eprintln!("Failed to get providers: {:?}", err);
                }
                QueryResult::GetRecord(Ok(ok)) => {
                    for PeerRecord {
                        record: Record { key, value, ..}, ..
                    } in ok.records{
                        println!("Got record {:?} {:?}", std::str::from_utf8(key.as_ref()).unwrap(), std::str::from_utf8(&value).unwrap());
                    }
                }
                QueryResult::GetRecord(Err(err)) => {
                    eprintln!("Failed to get record: {:?}", err);
                }
                QueryResult::PutRecord(Ok(PutRecordOk {key})) => {
                    println!("Successfully put record {:?}", std::str::from_utf8(key.as_ref()).unwrap());
                }
                QueryResult::PutRecord(Err(err)) => {
                    eprintln!("Failed to put record {:?}", err);
                }
                QueryResult::StartProviding(Ok(AddProviderOk{key})) => {
                    println!("Successfully put provider record {:?}", std::str::from_utf8(key.as_ref()).unwrap());
                }
                QueryResult::StartProviding(Err(err)) => {
                    eprintln!("Failed to put provider record: {:?}", err);
                }
                QueryResult::GetClosestPeers(Ok(ok)) => {
                    for peer in ok.peers{
                        println!("Closest peer: {:?}", peer);
                    }
                }
                QueryResult::GetClosestPeers(Err(err)) => {
                    eprintln!("Failed to get closest peers: {:?}", err);
                }
                _ => {}
            },
            _ => {}
        }
    }
   }
   let mut swarm = {
    let store = MemoryStore::new(local_peer_id);
    let kademlia = Kademlia::new(local_peer_id, store);
    let mdns = task::block_on(Mdns::new(MdnsConfig::default()))?;
    let behaviour = MyBehaviour{kademlia, mdns};
    Swarm::new(transport, behaviour, local_peer_id)
   };
   let mut stdin = io::BufReader::new(io::stdin()).lines().fuse();
   swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;
   loop{
    select!{
        line = stdin.select_next_some() => handle_input_line(&mut swarm.behaviour_mut().kademlia, line.expect("Stdin not to close")),
        event = swarm.select_next_some() => match event{
            SwarmEvent::NewListenAddr {address, ..} =>{
                println!("Listening in {:?}", address);
            }
            _ => {}
        }
    }
   }
}

fn handle_input_line(kademlia: &mut Kademlia<MemoryStore>, line: String){
    let mut args = line.split(' ');
    match args.next(){
        Some("GET") => {
            let key = {
                match args.next(){
                    Some(key) => Key::new(&key),
                    None => {
                        eprintln!("Expected key");
                        return;
                    }
                }
            };
            kademlia.get_record(key, Quorum::One);
        }
        Some("GET_PROVIDERS") => {
            let key = {
                match args.next(){
                    Some(key) => Key::new(&key),
                    None => {
                        eprintln!("Expected key");
                        return;
                    }
                }
            };
            kademlia.get_providers(key);
        }
        Some("PUT") => {
            let key = {
                match args.next(){
                    Some(key) => Key::new(&key),
                    None => {
                        eprintln!("Expected key");
                        return;
                    }
                }
            };
            let value = {
                match args.next(){
                    Some(value) => value.as_bytes().to_vec(),
                    None => {
                        eprintln!("Expected value");
                        return;
                    }
                }
            };
            let record = Record{key, value, publisher: None, expires: None};
            kademlia.put_record(record, Quorum::One).expect("Failed to store record locally");
        }
        Some("PUT_PROVIDER") => {
            let key = {
                match args.next() {
                    Some(key) => Key::new(&key),
                    None => {
                        eprintln!("Expected key");
                        return;
                    }
                }
            };
            kademlia
                .start_providing(key)
                .expect("Failed to start providing key");
        }
        Some("CLOSEST_PEERS") => {
            let key = {
                match args.next(){
                    Some(key) => Key::new(&key).to_vec(),
                    None => {
                        eprintln!("Expected key");
                        return;
                    }
                }
            };
            kademlia.get_closest_peers(key);
        }
        _ => {
            eprintln!("expected GET, GET_PROVIDERS, PUT, PUT_PROVIDER or CLOSEST_PEERS");
        }
    }
}