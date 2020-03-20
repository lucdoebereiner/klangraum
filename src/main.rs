extern crate jack;
extern crate crossbeam_channel;
extern crate native_tls;
extern crate tungstenite;

use tungstenite::{accept, Message};
use std::env;
use crossbeam_channel::{Sender, Receiver};
use crossbeam_channel::bounded;
use std::thread::spawn;
use native_tls::{Identity, TlsAcceptor, TlsStream};
use std::fs::File;
use std::io::{Read};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;



struct ClientBuffer {
    current_buffer: [f32; 16384],
    next_buffers: Vec<[f32; 16384]>,
    idx: usize,
    init_wait : bool,
    connection_id: u32
}

enum ClientUpdate {
    NewBuffer { connection_id: u32, buffer: [f32; 16384] },
    Closed { connection_id: u32 }
}


fn main() {

    let args: Vec<String> = env::args().collect();
    println!("Starting server on {}", args[1]);

    let mut client_buffers: Vec<ClientBuffer> = Vec::new();
    
    let (tx, rx): (Sender<ClientUpdate>, Receiver<ClientUpdate>)  = bounded(100);
    
    let (client, _status) =
        jack::Client::new("rust_jack", jack::ClientOptions::NO_START_SERVER).unwrap();

    let mut out_port = client
        .register_port("out_1", jack::AudioOut::default())
        .unwrap();

    const BUFFER_LENGTH: usize = 16384;

    let process = jack::ClosureProcessHandler::new(
        move |_: &jack::Client, ps: &jack::ProcessScope| -> jack::Control {
            // Get output buffer
            let out = out_port.as_mut_slice(ps);

            // Write output
            for v in out.iter_mut() {

		// check for new input
		while let Ok(update) = rx.try_recv() {
		    match update {
			ClientUpdate::NewBuffer { connection_id, buffer } => {		    
			    let idx = client_buffers.iter()
				.position(|b| b.connection_id == connection_id);
			    match idx {
				Some(i) => 
				    client_buffers[i].next_buffers.push(buffer),
				None => {
				    client_buffers.push(
					ClientBuffer {
					    current_buffer: buffer,
					    next_buffers: Vec::with_capacity(10),
					    idx: 0,
					    init_wait: true,
					    connection_id: connection_id,
					});
				    println!("new client: {}, total number: {}",
					     connection_id, client_buffers.len());

				}
			    }
			}
			ClientUpdate::Closed { connection_id } => {
			    let idx = client_buffers.iter()
				.position(|b| b.connection_id == connection_id);
			    match idx {
				Some(i) => {
				    client_buffers.remove(i);
				    ()
				},
				None => ()
			    }
			}
		    }
		}
			

		// sum and output
		let sum = client_buffers.iter()
		    .fold(0f32, |sum, b|
			  if b.init_wait {
			      sum
			  } else {
			      sum + b.current_buffer[b.idx]
			  });
                *v = sum;

		// update
		for i in 0..client_buffers.len() {
		    if client_buffers[i].init_wait {
			if client_buffers[i].next_buffers.len() > 4 {
			    client_buffers[i].init_wait = false;
			}
		    } else {
		    
			if client_buffers[i].idx >= BUFFER_LENGTH - 1 {
			    
			    match client_buffers[i].next_buffers.pop() {
				Some(next) => client_buffers[i].current_buffer = next,
				None => ()
			    };
			    
			    client_buffers[i].idx = 0;
			} else {
			    client_buffers[i].idx = (client_buffers[i].idx + 1) % BUFFER_LENGTH;
			}
		    }
		}

		
	    }

	    
            // Continue as normal
            jack::Control::Continue
        },
    );

    let _active_client = client.activate_async((), process).unwrap();

    let mut file = File::open(args[2].clone()).unwrap();
    let mut identity = vec![];
    file.read_to_end(&mut identity).unwrap();
    let identity = Identity::from_pkcs12(&identity, &args[3]).unwrap();

    let acceptor = TlsAcceptor::new(identity).unwrap();
    let acceptor = Arc::new(acceptor);

    let server = TcpListener::bind(args[1].clone()).unwrap();
    
    let mut id_counter = 0;
    for stream in server.incoming() {
        match stream {
            Ok(stream) => {
                let acceptor = acceptor.clone();
		let tx1 = tx.clone();
                spawn(move || {
                    let stream = acceptor.accept(stream).unwrap();
                    handle_client(stream, id_counter, tx1);
                });
		id_counter += 1;
            }
            Err(e) => { println!("error, {:?}", e) }
        }
    }
	
    fn handle_client(stream: TlsStream<TcpStream>, connection_id: u32, tx1: Sender<ClientUpdate>) -> () {
	let mut websocket = accept(stream).unwrap();
        loop {
	    let msg = websocket.read_message().unwrap();

	    match msg {
		Message::Binary(vec) => {
			
		    let buffer_slice = unsafe {
			std::slice::from_raw_parts_mut(vec.as_ptr() as *mut f32, vec.len() / 4)
		    };
		    
		    let mut buffer: [f32; 16384] = [0.0; 16384];
		    buffer.copy_from_slice(&buffer_slice[0..16384]);
		    
		    tx1.send(ClientUpdate::NewBuffer { connection_id: connection_id,
						       buffer: buffer }).unwrap();
		    
		}
		    
		Message::Close(_) => {
		    tx1.send(ClientUpdate::Closed { connection_id: connection_id }).unwrap();
		    ()
		}
		_ => ()
	    }	
        }
    }
    
		  
} 



