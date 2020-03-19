extern crate ws;
extern crate jack;
extern crate crossbeam_channel;
extern crate openssl;

use ws::{listen, Handler, Message, Result, CloseCode};
use std::env;
use crossbeam_channel::{Sender, Receiver};
use crossbeam_channel::bounded;



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


struct Server {
    out: ws::Sender,
    tx: Sender<ClientUpdate>
}

impl Handler for Server {

    fn on_message(&mut self, msg: Message) -> Result<()> {

	let connection_id = self.out.connection_id();
	     
	match msg {
	    ws::Message::Binary(vec) => {
		
		let buffer_slice = unsafe {
		    std::slice::from_raw_parts_mut(vec.as_ptr() as *mut f32, vec.len() / 4)
		};
		
		let mut buffer: [f32; 16384] = [0.0; 16384];
		buffer.copy_from_slice(&buffer_slice[0..16384]);
		
		self.tx.send(ClientUpdate::NewBuffer { connection_id: connection_id,
						       buffer: buffer }).unwrap();
		
		Ok(())
		    
	    }
	    _ => Ok(())	
	}	
    }


    fn on_close(&mut self, _code: CloseCode, _reason: &str) {

	let connection_id = self.out.connection_id();

	println!("Client {} closed the connection", connection_id);
	
	self.tx.send(ClientUpdate::Closed { connection_id: connection_id }).unwrap();
    }

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

    listen(args[1].clone(), |out| { Server { out: out, tx: tx.clone() } } ).unwrap();
		  
} 



