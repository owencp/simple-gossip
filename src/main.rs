use log::LevelFilter::Info;
use tentacle::{
    builder::{MetaBuilder, ServiceBuilder},
    bytes::Bytes,
    context::{ProtocolContext, ProtocolContextMutRef, ServiceContext},
    secio::SecioKeyPair,
    service::{ProtocolHandle, ServiceEvent, TargetProtocol, ServiceControl},
    traits::{ServiceHandle, ServiceProtocol},
    SessionId,
};

use std::{
    collections::HashMap,
    str::FromStr,
    thread,
    time::Duration,
};
use serde::{Deserialize, Serialize};
use crossbeam_channel::{unbounded,Sender,Receiver};
//TODO: begin
const SWITCH_ROUND_TOKEN: u64 = 1;
/*
const STAR_NUMBER: usize = 3;
const INTERVAL_SECS: usize = 10;
*/
//TODO: end
static mut G_CONTROL: Option<ServiceControl> = None;

//CLI args
struct CliArgs {
    id: u8,
    port: u16,
    bootnode: Option<String>,
}

impl Default for CliArgs {
    fn default() -> Self {
        Self {
            id: 0,
            port: 1234,
            bootnode: None,
        }
    }
}

//Parses the command line args.
fn parse_args() -> CliArgs {
    let mut parsed_args = CliArgs::default();
    let args: Vec<_> = std::env::args().collect();

    if args.len() < 2 {
        log::info!("cli args must be not less than 2: cargo run <node_id> <port_num>");
    }
    {
        use std::str::FromStr;
        parsed_args.id = u8::from_str(&args[1]).expect("node id");
        parsed_args.port = u16::from_str(&args[2]).expect("port number");
    }
    if args.len() > 3 && !args[3].is_empty() {
        parsed_args.bootnode = Some(args[3].clone());
    }

    parsed_args
}

//define p2p peers
#[derive(Serialize, Deserialize, Debug, Clone)]
struct PeerInfo {
    node_id: u8,
    port_id: u16,
    peer_id: String,
}

//define p2p message
#[derive(Serialize, Deserialize, Debug)]
struct Peers {
    conns:Vec<PeerInfo>,
}

#[derive(Serialize, Deserialize, Debug)]
enum Message {
    PeerInfo(PeerInfo),
    Peers(Peers),
    Records(usize), //TODO
}

struct Actor{
    node_id: u8,
    port_id: u16,
    peer_id: String,
    peer_session:HashMap<String, SessionId>, //store peer<-->session pair
    half_conns:HashMap<String, PeerInfo>,  //store the peers received from other dialed peer
    full_conns:HashMap<String, PeerInfo>, //store the diaied peers
    records:Vec<usize>,
    peer_sender: Sender<u16>,
}

impl Actor {
    pub fn new(node_id:u8, port_id: u16, peer_sender:Sender<u16>) -> Self {
        Self {
            node_id,
            port_id,
            peer_id: "default".to_string(),
            peer_session: HashMap::new(),
            half_conns: HashMap::new(),   
            full_conns: HashMap::new(),
            records: Vec::new(),
            peer_sender,
        }
    }
    
    pub fn set_self_peer(&mut self, peer_id:String) {
        if self.peer_id == "default".to_string() {
            self.peer_id = peer_id;
            log::info!("set self peer_id {:?}", self.peer_id);
        }
    }
    
    pub fn add_session(&mut self, peer_id: String, s_id: SessionId) {
        self.peer_session.insert(peer_id, s_id);
    }
    pub fn add_half_peer(&mut self, peer_info: PeerInfo) {
        self.half_conns.insert(
            peer_info.peer_id.clone(),
            peer_info);
    }

    pub fn add_full_peer(&mut self, peer_info: PeerInfo) {
        self.full_conns.insert(
            peer_info.peer_id.clone(), 
            peer_info.clone(),
            );

        //remove half connection
        self.half_conns.remove(&peer_info.peer_id);
    }
    
    pub fn disconnect_peer(&mut self, peer_id: String) {
        //remove peer from half and full peer
        self.half_conns.remove(&peer_id);
        self.full_conns.remove(&peer_id);
    }
}
    
impl ServiceProtocol for Actor {
    fn init(&mut self, context: &mut ProtocolContext) {
        if context.proto_id == 0.into() {
            let _ = context.set_service_notify(0.into(), Duration::from_secs(10), SWITCH_ROUND_TOKEN);
        }
    }
    
    fn notify(&mut self, context: &mut ProtocolContext, token: u64) {
        match token {
            SWITCH_ROUND_TOKEN => {
                //TODO:
            }
            _ => unreachable!(),
        }
    }

    fn connected(&mut self, context: ProtocolContextMutRef<'_>, version: &str) {
        let session = context.session;
        log::info!("p2p-message: receive incoming connection {}", session.address);
        
        let self_peer_id = 
            context
            .key_pair()
            .expect("secio")
            .peer_id()
            .to_base58();
        
        self.set_self_peer(self_peer_id.clone());
        //store peer seeion

        let remote_peer_id = 
            session
            .remote_pubkey
            .as_ref()
            .expect("secio")
            .peer_id()
            .to_base58();
        
        self.add_session(remote_peer_id.clone(), session.id);
        
        //send PeerInfo message
        let msg = Message::PeerInfo(PeerInfo {
            node_id:self.node_id.clone(),
            port_id:self.port_id.clone(),
            peer_id:self.peer_id.clone(),
        });
        let msg_bytes = Bytes::from(serde_json::to_vec(&msg).expect("serialize to JSON"));
        context.send_message(msg_bytes).expect("send peer info");
        log::info!("send self info: node_id {} port_id {} peer_id {:?} to  peer {:?}", 
            self.node_id, self.port_id, self.peer_id, remote_peer_id);
        //send Peers message to this peer
        let peers: Vec<_> = self
            .full_conns
            .clone()
            .into_iter()
            .map(|(_, _info)| _info.clone())
            .collect();
        if peers.len() > 0 {
            log::info!("send peers {:?} to peer {:?}", peers, remote_peer_id);
            let msg = Message::Peers(Peers {
                conns: peers
            });
        
            let msg_bytes = Bytes::from(serde_json::to_vec(&msg).expect("serialize to JSON"));
            context.send_message(msg_bytes).expect("send peers");
        }
        
    }

    fn disconnected(&mut self, context: ProtocolContextMutRef<'_>) {
        let session = context.session;
        log::info!("p2p-message: disconnected from {}", session.address);

        let remote_peer_id = 
            session
            .remote_pubkey
            .as_ref()
            .expect("secio")
            .peer_id()
            .to_base58();

        self.disconnect_peer(remote_peer_id.clone());  
        
        log::info!("remove peer {:?}", remote_peer_id);      
    }

    fn received(&mut self, context: ProtocolContextMutRef<'_>, data: Bytes) {
        let session = context.session;
        let msg_bytes: serde_json::Result<Message> = serde_json::from_slice(&data);
        if let Ok(msg) = msg_bytes {
            log::info!(
                "p2p-message received from {}: {:?}",
                session.address,
                msg
            );
 
            match msg {
                Message::Peers(peers) => {
                    let remmote_peer_id = session.remote_pubkey.as_ref().expect("secio").peer_id().to_base58();
                    let self_peer_id = context.key_pair().expect("secio").peer_id().to_base58();
                    let _peers:Vec<PeerInfo> = 
                        peers
                        .conns
                        .into_iter()
                        .filter(|peer_info| peer_info.peer_id != self_peer_id)
                        .collect();
                    
                    //store peers to half_conns, and dial these peers
                    for _info in _peers {
                        if !self.full_conns.contains_key(&_info.peer_id) {
                            self.add_half_peer(_info.clone());
                            //send to channel to dial the peer
                            self.peer_sender.send(_info.port_id).unwrap();
                        }
                    }
                }
                Message::PeerInfo(peer_info) => {
                    let remmote_peer_id = session.remote_pubkey.as_ref().expect("secio").peer_id().to_base58();
                    let session_id = session.id;
                    self.add_full_peer(peer_info);
                    self.add_session(remmote_peer_id, session_id);
                }
                _ => {
                }
            }
        } 
    }
}

//define app handle
struct AppHandle;

impl ServiceHandle for AppHandle {
    fn handle_event(&mut self, control: &mut ServiceContext, event: ServiceEvent) {
        if let ServiceEvent::ListenStarted { address: _ } = event {
            log::info!("Start listen...");
        }
    }
}

fn main() {
    //init logger
    env_logger::builder().filter_level(Info).init();

    let args = parse_args();

    log::info!("args - node_id is {}, port is {}, bootnode is {:?}", 
       args.id, args.port, args.bootnode);
    
    let node_id = args.id.clone();    
    let port_id = args.port.clone();
    //new actor
    let (_sender, _receiver) = unbounded();
    let actor = Box::new(Actor::new(node_id, port_id,  _sender));
    let mut rt = tokio::runtime::Runtime::new().expect("create tokio runtime");
    
    thread::spawn( move || {
    rt.block_on(async {
        let key_pair = SecioKeyPair::secp256k1_generated();
        log::info!(
            "listen on /ip4/127.0.0.1/tcp/{}/p2p/{}",
            args.port,
            key_pair.peer_id().to_base58()
        );

        let protocol_meta = MetaBuilder::new()
            .id(0.into())
            .service_handle(move || {
                ProtocolHandle::Callback(actor)
            })
            .build();

        let mut app_service = ServiceBuilder::default()
            .insert_protocol(protocol_meta)
            .key_pair(key_pair)
            .timeout(std::time::Duration::new(60, 0))
            .build(AppHandle);
        
        app_service
            .listen(format!("/ip4/127.0.0.1/tcp/{}", args.port).parse().unwrap())
            .await
            .expect("listen ...");


        //connect to bootnode
        if let Some(bootnode) = args.bootnode {
            log::info!("dial {}", bootnode);
            app_service
                .dial(
                    bootnode.parse().expect("bootnode multiaddr"),
                    TargetProtocol::All,
                )
                .await
                .expect("connect bootnode");
        }
        unsafe {
            G_CONTROL = Some(app_service.control().clone());
        }

        {
            use futures::stream::StreamExt;
            while app_service.next().await.is_some() {
            }
        }
    });
    });

    loop {
        if let Ok(port) = _receiver.try_recv() {
            log::info!("dial port {}", port);
            //start nodes at same local device, the ip hardcode to 127.0.0.1
            let multi_peer = format!("/ip4/127.0.0.1/tcp/{}", port).parse().unwrap();
            unsafe {
                G_CONTROL.as_mut().map(|control| {
                    control.dial(multi_peer, TargetProtocol::All).unwrap()
                });
            }
        }
    }
}
