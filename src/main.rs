extern crate ws;
extern crate opus;
extern crate jack;
extern crate crossbeam_channel;

use ws::listen;
//use std::thread;
// use std::sync::mpsc;
use crossbeam_channel::{Sender, Receiver};
use crossbeam_channel::bounded;


struct ClientBuffer {
    current_buffer: [f32; 2048],
//    next_buffer: [f32; 2048],
    next_buffers: Vec<[f32; 2048]>,
    idx: usize,
    connection_id: u32
}

struct ClientUpdate {
    connection_id: u32,
    buffer: [f32; 2048]
}


fn main() {

    let mut client_buffers: Vec<ClientBuffer> = Vec::new();
    
    let (tx, rx): (Sender<ClientUpdate>, Receiver<ClientUpdate>)  = bounded(1_000);
    
    let (client, _status) =
        jack::Client::new("rust_jack", jack::ClientOptions::NO_START_SERVER).unwrap();

    let mut out_port = client
        .register_port("out_1", jack::AudioOut::default())
        .unwrap();

    const BUFFER_LENGTH: usize = 2048;

    let process = jack::ClosureProcessHandler::new(
        move |_: &jack::Client, ps: &jack::ProcessScope| -> jack::Control {
            // Get output buffer
            let out = out_port.as_mut_slice(ps);

            // Write output
            for v in out.iter_mut() {

		// check for new input
		while let Ok(update) = rx.try_recv() {
		    let idx = client_buffers.iter().position(|b| b.connection_id == update.connection_id);
		    match idx {
			Some(i) => client_buffers[i].next_buffers.push(update.buffer),
			None => client_buffers.push(
			    ClientBuffer {
				current_buffer: update.buffer,
				next_buffers: Vec::with_capacity(10),
				idx: 0,
				connection_id: update.connection_id,
			    } )
		    }
		}

		// sum and output
		let sum = client_buffers.iter().fold(0f32, |sum, b| sum + b.current_buffer[b.idx]);
                *v = sum;

		// update
		for i in 0..client_buffers.len() {
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

	    
            // Continue as normal
            jack::Control::Continue
        },
    );
    
    let _active_client = client.activate_async((), process).unwrap();

    
    listen("localhost:3012", |out| {
	let tx1 = tx.clone();
	 move |msg| {
            //out.send(msg)
	     let connection_id = out.connection_id();
	     
	    match msg {
		ws::Message::Binary(vec) => {
		    
		    let buffer_slice = unsafe {
			std::slice::from_raw_parts_mut(vec.as_ptr() as *mut f32, vec.len() / 4)
		    };

		    let mut buffer: [f32; 2048] = [0.0; 2048];
		    buffer.copy_from_slice(&buffer_slice[0..2048]);
		    
		    tx1.send(ClientUpdate { connection_id: connection_id,
					   buffer: buffer }).unwrap();
		    
		    Ok(())
			
		}
		_ => Ok(())
		    
	    }
	    
	}
    }).unwrap();

		  
} 



// static mut DECODER: Option<opus::Decoder> = None;

    // unsafe {vector
    // 	DECODER = Some(opus::Decoder::new(48000, opus::Channels::Mono).unwrap());
    // }
//		    println!("{:?}", n);
//		    println!("vec: {:?}", &vec[0..100]);
//		    println!("output: {:?}", &audio);
//		    println!("output0: {:?}", audio[0]);
//		    println!("output length: {}", audio.len());
// 		    println!("sum: {:?}", audio.iter()
// //			     .take(n.unwrap())
// 			     .cloned()
// 			     // .inspect(|x| if x.is_nan() {
// 			     // 	 println!("found nan: {}", x)
// 			     // })
// 			     .inspect(|x| 
// 				 println!("{}", x)
// 			     )
// 			     .fold(0f32, |sum, i| sum + i));

//		    let mut decoder = opus::Decoder::new(48000, opus::Channels::Mono).unwrap();
		    // let mut output = Vec::with_capacity(48000);
		    // output.resize(48000, 0f32);

		    // let mut n = Ok(0);
		    // unsafe {
		    // 	match &mut DECODER {
		    // 	    None => (),
		    // 	    Some(d) => n = d.decode_float(&vec, &mut output, false)
		    // 	};
		    // }

