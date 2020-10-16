use crossbeam_channel::bounded;
use crossbeam_channel::{Receiver, Sender};
use minimp3::Decoder;
use native_tls::{Identity, TlsAcceptor};
use std::collections::VecDeque;
use std::env;
use std::fs::File;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::thread::spawn;
use tungstenite::{accept, Message};

struct ClientBuffer {
    current_buffer: Vec<f32>,
    next_buffers: VecDeque<Vec<f32>>,
    idx: usize,
    init_wait: bool,
    connection_id: u32,
    decoder: Decoder<Mp3Buffer>,
}

struct Mp3Buffer {
    data: Vec<u8>,
}

impl Read for Mp3Buffer {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        let mut slice = unsafe { std::slice::from_raw_parts(self.data.as_ptr(), self.data.len()) };
        slice.read(buf)
    }
}

impl ClientBuffer {
    fn decode(&mut self, mp3_data: Vec<u8>) -> Result<Vec<f32>, minimp3::Error> {
        self.decoder.reader_mut().data = mp3_data;
        self.decoder.next_frame().map(|frame| {
            frame
                .data
                .into_iter()
                .map(|s| (s as f32) / (i16::max_value() as f32))
                .collect()
        })
    }

    fn process(&mut self) -> f32 {
        let mut output = 0.0;
        if !self.init_wait {
            output = self.current_buffer[self.idx];
            let length = self.current_buffer.len();
            if self.idx >= length - 1 {
                match self.next_buffers.pop_front() {
                    Some(next) => self.current_buffer = next,
                    None => (),
                };
                self.idx = 0;
            } else {
                self.idx = (self.idx + 1) % length
            };
        } else if self.next_buffers.len() > 4 {
            self.init_wait = false;
        }
        output
    }
}

enum ClientUpdate {
    NewBuffer { connection_id: u32, buffer: Vec<u8> },
    Closed { connection_id: u32 },
}

struct MsgToClient {
    id: u32,
    jack_input: usize,
}

fn handle_client<S: Read + Write>(
    mut websocket: tungstenite::WebSocket<S>,
    //    stream: TlsStream<TcpStream>,
    connection_id: u32,
    tx1: Sender<ClientUpdate>,
    receiver: Receiver<MsgToClient>,
) -> () {
    while let Ok(msg) = websocket.read_message() {
        match msg {
            Message::Binary(vec) => {
                tx1.send(ClientUpdate::NewBuffer {
                    connection_id: connection_id,
                    buffer: vec,
                })
                .unwrap();
            }
            Message::Close(_) => {
                tx1.send(ClientUpdate::Closed {
                    connection_id: connection_id,
                })
                .unwrap();
                ()
            }
            _ => (),
        }
        if let Ok(to_client) = receiver.try_recv() {
            websocket
                .write_message(Message::Text(format!(
                    "{{ \"id\": {}, \"channel\": {} }}",
                    to_client.id, to_client.jack_input
                )))
                .unwrap();
        }
    }
}

fn find_buffer_position(
    buffers: &[Vec<ClientBuffer>],
    connection_id: u32,
) -> Option<(usize, usize)> {
    let mut x = 0;
    let mut y = 0;
    let mut found = false;

    while !found && x < buffers.len() {
        let idx = buffers[x]
            .iter()
            .position(|b| b.connection_id == connection_id);
        match idx {
            Some(found_y) => {
                y = found_y;
                found = true
            }
            None => x += 1,
        }
    }

    if found {
        Some((x, y))
    } else {
        None
    }
}

fn next_free_index(buffers: &[Vec<ClientBuffer>]) -> usize {
    let lengths: Vec<usize> = buffers.iter().map(|v| v.len()).collect();
    let index = lengths
        .iter()
        .min()
        .and_then(|&smallest| lengths.iter().position(|&l| l == smallest));
    index.unwrap_or(0)
}

fn number_of_listeners(buffers: &[Vec<ClientBuffer>]) -> usize {
    buffers.iter().map(|v| v.len()).sum()
}

fn main() {
    let args: Vec<String> = env::args().collect();
    println!("Starting server on {}", args[1]);

    let (tx, rx): (Sender<ClientUpdate>, Receiver<ClientUpdate>) = bounded(100);

    let (sender_client_msg, receiver_client_msg): (Sender<MsgToClient>, Receiver<MsgToClient>) =
        bounded(100);

    let (client, _status) =
        jack::Client::new("klangraum_input", jack::ClientOptions::NO_START_SERVER).unwrap();

    let n_outs = args[2].parse::<usize>().unwrap();

    let mut client_buffers: Vec<Vec<ClientBuffer>> = Vec::with_capacity(n_outs);
    for _i in 0..n_outs {
        client_buffers.push(Vec::new())
    }

    let mut out_ports = Vec::new();

    for i in 1..(n_outs + 1) {
        let name = String::from(format!("out_{}", i));
        out_ports.push(
            client
                .register_port(&name, jack::AudioOut::default())
                .unwrap(),
        );
    }

    let process = jack::ClosureProcessHandler::new(
        move |_: &jack::Client, ps: &jack::ProcessScope| -> jack::Control {
            let mut log_file = match File::create("log.log") {
                Err(why) => panic!("couldn't create {}: {}", "log.log", why),
                Ok(file) => file,
            };

            // check for new input
            while let Ok(update) = rx.try_recv() {
                match update {
                    ClientUpdate::NewBuffer {
                        connection_id,
                        buffer,
                    } => {
                        let idx = find_buffer_position(&client_buffers, connection_id);
                        match idx {
                            Some((x, y)) => {
                                let decoded = client_buffers[x][y].decode(buffer);
                                match decoded {
                                    Ok(dec) => client_buffers[x][y].next_buffers.push_back(dec),
                                    _ => (),
                                }
                            }
                            None => {
                                let decoder = Decoder::new(Mp3Buffer { data: vec![] });
                                let mut client_buffer = ClientBuffer {
                                    current_buffer: vec![],
                                    next_buffers: VecDeque::with_capacity(10),
                                    idx: 0,
                                    init_wait: true,
                                    connection_id: connection_id,
                                    decoder: decoder,
                                };

                                let decoded = client_buffer.decode(buffer);
                                match decoded {
                                    Ok(dec) => {
                                        client_buffer.current_buffer = dec;
                                        let index = next_free_index(&client_buffers);
                                        client_buffers[index].push(client_buffer);
                                        sender_client_msg
                                            .send(MsgToClient {
                                                id: connection_id,
                                                jack_input: index,
                                            })
                                            .unwrap();
                                    }
                                    _ => (),
                                }
                                //
                                println!(
                                    "new client: {}, total number: {}",
                                    connection_id,
                                    client_buffers.len()
                                );
                                match write!(log_file, "{}\n", number_of_listeners(&client_buffers))
                                {
                                    _ => (),
                                };
                                let _ = log_file.flush();
                            }
                        }
                    }
                    ClientUpdate::Closed { connection_id } => {
                        let idx = find_buffer_position(&client_buffers, connection_id);
                        match idx {
                            Some((x, y)) => {
                                client_buffers[x].remove(y);
                                match write!(log_file, "{}\n", number_of_listeners(&client_buffers))
                                {
                                    _ => (),
                                };
                                let _ = log_file.flush();
                            }
                            None => (),
                        }
                    }
                }
            }

            for out_i in 0..n_outs {
                // Get output buffer
                let out = out_ports[out_i].as_mut_slice(ps);
                for v in out.iter_mut() {
                    // sum and output
                    let sum = client_buffers[out_i]
                        .iter_mut()
                        .fold(0f32, |acc, buf| acc + buf.process());
                    *v = sum;
                }
            }

            // Continue as normal
            jack::Control::Continue
        },
    );

    let _active_client = client.activate_async((), process).unwrap();

    let mut acceptor = None;

    if args.len() > 4 {
        let mut file = File::open(args[3].clone()).unwrap();
        let mut identity = vec![];
        file.read_to_end(&mut identity).unwrap();
        let identity = Identity::from_pkcs12(&identity, &args[4]).unwrap();

        let tls_acceptor = TlsAcceptor::new(identity).unwrap();
        acceptor = Some(Arc::new(tls_acceptor));
    }

    let server = TcpListener::bind(args[1].clone()).unwrap();

    let mut id_counter = 0;
    for stream in server.incoming() {
        match stream {
            Ok(stream) => {
                let acceptor = acceptor.clone();
                let tx1 = tx.clone();
                let receiver = receiver_client_msg.clone();
                spawn(move || {
                    match acceptor {
                        Some(acceptor) => {
                            let ws = accept(acceptor.accept(stream).unwrap()).unwrap();
                            handle_client(ws, id_counter, tx1, receiver)
                        }
                        None => {
                            let ws = accept(stream).unwrap();
                            handle_client(ws, id_counter, tx1, receiver);
                        }
                    };
                });
                id_counter += 1;
            }
            Err(e) => println!("error, {:?}", e),
        }
    }
}
