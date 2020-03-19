extern crate ws;
extern crate opus;
extern crate jack;
extern crate crossbeam_channel;

use ws::listen;
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

struct ClientUpdate {
    connection_id: u32,
    buffer: [f32; 16384]
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
		    let idx = client_buffers.iter()
			.position(|b| b.connection_id == update.connection_id);
		    match idx {
			Some(i) => client_buffers[i].next_buffers.push(update.buffer),
			None => client_buffers.push(
			    ClientBuffer {
				current_buffer: update.buffer,
				next_buffers: Vec::with_capacity(10),
				idx: 0,
				init_wait: true,
				connection_id: update.connection_id,
			    } )
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

    
    listen(args[1].clone(), |out| {
	let tx1 = tx.clone();
	 move |msg| {
            //out.send(msg)
	     let connection_id = out.connection_id();
	     
	    match msg {
		ws::Message::Binary(vec) => {
		    
		    let buffer_slice = unsafe {
			std::slice::from_raw_parts_mut(vec.as_ptr() as *mut f32, vec.len() / 4)
		    };

		    let mut buffer: [f32; 16384] = [0.0; 16384];
		    buffer.copy_from_slice(&buffer_slice[0..16384]);
		    
		    tx1.send(ClientUpdate { connection_id: connection_id,
					   buffer: buffer }).unwrap();
		    
		    Ok(())
			
		}
		_ => Ok(())
		    
	    }
	    
	}
    }).unwrap();

		  
} 



