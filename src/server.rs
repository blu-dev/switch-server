use std::collections::VecDeque;
use std::io::{self, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::mpsc::{self, Sender, Receiver, TryRecvError, SendError};
use std::thread::{
    self,
    JoinHandle
};
use byteorder::{LittleEndian, WriteBytesExt, ReadBytesExt};
use owo_colors::OwoColorize;
use thiserror::Error;

use std::time::Duration;

use parking_lot::RwLock;
use crate::packets::ConnectionAcquired;

use super::*;

/// Server commands to be used with [ServerInfo] in order to send information to the server
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ServerCommand {
    /// Pauses the server's operation until [ServerCommand::Resume] is encountered. If it is already paused, this does nothing
    Pause,
    /// Resumes the server's operation if it is paused, if it is not paused this does nothing
    Resume,
    /// Kills the server, does not matter if the server is paused or currently running
    Kill
}

/// Server events that are sent from a [Server] to a [ServerInfo] in order to provide information about the state of the server
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ServerEvent {
    /// Signals that the server has connected to a client
    Connected
}

#[derive(Error, Debug)]
pub enum PacketError {
    #[error("Packet queue is currently locked")]
    Locked,
    #[error("The packet queue is currently empty")]
    Empty
}

pub struct Server;

type PacketType = super::Packet;

#[allow(non_camel_case_types)]
type TS_PacketQueue = Arc<RwLock<VecDeque<PacketType>>>;

/// A container that helps manage the state of the spawned server.
pub struct ServerInfo {
    thread_join_handle: Option<JoinHandle<()>>,
    incoming_packet_queue: TS_PacketQueue,
    outgoing_packet_queue: TS_PacketQueue,
    cmd_channel: Sender<ServerCommand>,
    event_channel: Receiver<ServerEvent>
}

unsafe impl Sync for ServerInfo {}
unsafe impl Send for ServerInfo {}

impl Drop for ServerInfo {
    fn drop(&mut self) {
        if self.thread_join_handle.is_none() {
            return;
        }
        
        if let Err(e) = self.send_packet(Packet::Disconnection(packets::Disconnection::default())) {
            error!("Failed to send disconnection packet to peer! Error: {:?}", e);
        }
        
        // We don't care about the error here since it's it will just be what we pass in (ServerCommand::Kill)
        if self.cmd_channel.send(ServerCommand::Kill).is_err() {
            error!("Failed to send Kill command to server because it has already disconnected!");
        }
    }
}

impl ServerInfo {
    /// Sends a [ServerCommand] to the server
    pub fn send_command(&self, command: ServerCommand) {
        if let Err(SendError(cmd)) = self.cmd_channel.send(command) {
            error!("Failed to send command '{:?}' to server because the server has disconnected!", cmd);
        }
    }

    /// Sends a packet while also blocking the thread until it can
    pub fn send_packet_blocking(&self, packet: PacketType) {
        let mut queue = self.outgoing_packet_queue.write();
        queue.push_back(packet);
    }

    /// Sends packets while also blocking the thread until it can
    pub fn send_packets_blocking(&self, packets: Vec<PacketType>) {
        let mut queue = self.outgoing_packet_queue.write();
        queue.extend(packets.into_iter());
    }

    /// Sends a [PacketType] to the client through the server
    pub fn send_packet(&self, packet: PacketType) -> Result<(), PacketError> {
        if let Some(mut queue) = self.outgoing_packet_queue.try_write() {
            queue.push_back(packet);
            Ok(())
        } else {
            Err(PacketError::Locked)
        }
    }

    /// Sends multiple [PacketType]s to the client through the server
    pub fn send_packets(&self, packets: Vec<PacketType>) -> Result<(), PacketError> {
        if let Some(mut queue) = self.outgoing_packet_queue.try_write() {
            queue.extend(packets.into_iter());
            Ok(())
        } else {
            Err(PacketError::Locked)
        }
    }

    /// Gets an incoming packet, blocking the thread until it can
    pub fn get_packet_blocking(&self) -> Result<PacketType, PacketError> {
        let mut queue = self.incoming_packet_queue.write();
        match queue.pop_front() {
            Some(packet) => Ok(packet),
            None => Err(PacketError::Empty)
        }
    }

    /// Gets all incoming packets, blocking the thread until it can
    pub fn get_incoming_blocking(&self) -> Result<VecDeque<PacketType>, PacketError> {
        let mut queue = self.incoming_packet_queue.write();
        let mut empty = VecDeque::new();
        std::mem::swap(&mut empty, &mut queue);
        drop(queue);
        if empty.is_empty() {
            Err(PacketError::Empty)
        } else {
            Ok(empty)
        }
    }

    /// Tries to receive an incoming [PacketType]
    pub fn get_packet(&self) -> Result<PacketType, PacketError> {
        if let Some(mut queue) = self.incoming_packet_queue.try_write() {
            match queue.pop_front() {
                Some(packet) => Ok(packet),
                None => Err(PacketError::Empty)
            }
        } else {
            Err(PacketError::Locked)
        }
    }

    /// Tries to get all incoming packets
    pub fn get_incoming(&self) -> Result<VecDeque<PacketType>, PacketError> {
        if let Some(mut queue) = self.incoming_packet_queue.try_write() {
            let mut empty = VecDeque::new();
            std::mem::swap(&mut *queue, &mut empty);
            drop(queue);
            if empty.is_empty() {
                Err(PacketError::Empty)
            } else {
                Ok(empty)
            }
        } else {
            Err(PacketError::Locked)
        }
    }

    /// Attempts to get an event from the server
    pub fn get_event(&self) -> Result<ServerEvent, TryRecvError> {
        self.event_channel.try_recv()
    }

    /// Performs a connection handshake between server and client. This blocks the thread until both are connected
    pub fn ensure_connection(&self, is_client: bool) -> anyhow::Result<()> {
        // Send the connection packet, abstracted since we have different order
        // for client
        fn send_connection_packet(this: &ServerInfo) -> anyhow::Result<()> {
            let packet = Packet::Connection(ConnectionAcquired::new());
            Ok(this.send_packet(packet)?)
        }

        // Get the connection packet, abstracted since we have different order
        // for client
        fn get_connection_packet(this: &ServerInfo) -> anyhow::Result<()> {
            loop {
                // Block on the RwLock for the incoming packets
                // If the packet is not empty AND it is not connection, then
                // we hve the order of operations wrong
                match this.get_packet_blocking() {
                    Ok(Packet::Connection(_)) => break,
                    Err(PacketError::Empty) => {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    },
                    _ => {
                        return Err(io::Error::new(io::ErrorKind::InvalidData, "First message received from peer was not the connection message!").into())
                    }
                }
            }

            Ok(())
        }

        info!("Ensuring connection to peer...");

        // Wait for the connection event to be triggered by the server
        loop {
            match self.get_event() {
                // On connection event we can exit this loop
                Ok(ServerEvent::Connected) => break,
                // If we have already disconnected it means that the sender has
                // been dropped/closed
                Err(TryRecvError::Disconnected) => {
                    return Err(io::Error::new(io::ErrorKind::BrokenPipe, "Failed to connect to peer because the server has already stopped").into())
                },
                _ => {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        }

        if is_client {
            get_connection_packet(self)?;
            send_connection_packet(self)?;
        } else {
            send_connection_packet(self)?;
            get_connection_packet(self)?;
        }

        info!("Connection to peer successful!");

        Ok(())
    }

    /// Closes the server and waits on the thread
    pub fn close(mut self) {
        if let Err(e) = self.send_packet(Packet::Disconnection(packets::Disconnection::default())) {
            error!("Failed to send disconnection packet to peer! Error: {:?}", e);
        }

        // We don't care about the error here since it's it will just be what we pass in (ServerCommand::Kill)
        if self.cmd_channel.send(ServerCommand::Kill).is_err() {
            error!("Failed to send Kill command to server because it has already disconnected!");
        }

        if let Some(handle) = self.thread_join_handle.take() {
            if let Err(e) = handle.join() {
                error!("Failed to wait on server thread. Reason: {:?}", e);
            }
        }
    }
}

impl Server {
    /// Constructs an empty [TS_PacketQueue]
    fn create_queue() -> TS_PacketQueue {
        Arc::new(RwLock::new(VecDeque::new()))
    }

    /// Attempts to open the client specified and returns the stream
    fn open_client(client: TcpListener) -> anyhow::Result<TcpStream> {
        // Attempt to conenct to the client and get it's stream + socket
        // then log out the socket
        let (stream, socket_addr) = client.accept()?;

        info!("Connecting to client {}", socket_addr);

        Ok(stream)
    }

    fn send_event(event_tx: &Sender<ServerEvent>, event: ServerEvent) -> anyhow::Result<()> {
        // Try sending the event, if it fails then log it and return it still
        event_tx.send(event)
            .map_err(|SendError(e)| {
                error!("Failed to send event '{:?}' because the receiver has disconnected!", event);
                SendError(e)
            })?;
        
        Ok(())
    }

    /// Handles sending the outgoing packet
    fn send_outgoing(queue: VecDeque<PacketType>, stream: &mut TcpStream) -> anyhow::Result<()> {
        info!("Sending {} packets!", queue.len());

        let mut result = Ok(());

        let len = queue.len();
        let mut sent = 0;
        for (idx, packet) in queue.into_iter().enumerate() {
            info!(target: "print", "Sending packet {}... ", idx + 1);

            let mut writer = io::Cursor::new(vec![]);

            // Try to write the data into a Vec
            // We can't write directly into the TcpStream because of a requirement on Seek
            let data = match packet.encode(&mut writer) {
                Ok(_) => writer.into_inner(),
                Err(e) => {
                    info!(target: "no-path", "{} Aborting rest of packets...", "Failed!".red());
                    result = Err(e);
                    break;
                }
            };

            // Write the size of the data to the TcpStream so we know how large the incoming packet is
            let len = data.len() as u64;
            let mut sized_data = vec![];
            if let Err(e) = sized_data.write_u64::<LittleEndian>(len) {
                info!(target: "no-path", "{} Aborting rest of packets...", "Failed!".red());
                result = Err(e);
                break;
            }
            sized_data.extend(data.into_iter());

            // Write the data to the TcpStream
            match stream.write(&sized_data) {
                Ok(_) => info!(target: "no-path", "{}", "Success!".green()),
                Err(e) => {
                    info!(target: "no-path", "{} Aborting rest of packets...", "Failed!".red());
                    result = Err(e);
                    break;
                }
            }

            // increment successful packets
            sent = idx + 1;
        }

        info!("Sent {}/{} packets!", sent, len);
        
        Ok(result?)
    }

    fn get_incoming(stream: &mut TcpStream) -> io::Result<PacketType> {
        // Set the stream to non blocking only temporarily to see if there is any data in the buffer
        // Once there is data in the buffer, we are going to set the stream to blocking and block on the incoming packet
        stream.set_nonblocking(true)?;

        // Unfortunately the skyline toolchain doesn't quite accommodate io::ErrorKind::WouldBlock so I just assume
        if stream.peek(&mut [0u8]).is_err() {
            stream.set_nonblocking(false)?;
            return Err(io::Error::new(io::ErrorKind::WouldBlock, "Resource would block"))
        }

        stream.set_nonblocking(false)?;

        // Get the size of the incoming packet
        let mut le_len = vec![0u8; 8];
        stream.read_exact(&mut le_len)?;

        let packet_length = io::Cursor::new(le_len).read_u64::<LittleEndian>()? as usize;

        info!(target: "print", "Encountered incoming packet with length {}. Reading... ", packet_length);

        // Read in the packet data
        let mut packet_data = vec![0u8; packet_length];
        stream.read_exact(&mut packet_data)?;

        info!(target: "no-path", "{}", "Success!".green());

        // Try to decode the packet data
        Packet::decode(&mut io::Cursor::new(packet_data))
    }

    /// Gets the [TcpStream] as a client. Same functionality, but someone else is hosting
    fn get_stream_as_client(server_ip: &str) -> anyhow::Result<TcpStream> {
        loop {
            thread::sleep(Duration::from_millis(100));
            if let Ok(c) = TcpStream::connect(&format!("{}:{}", server_ip, crate::PORT)) {
                break Ok(c);
            }
        }
    }

    /// Gets the [TcpStream] as a server.
    fn get_stream_as_server() -> anyhow::Result<TcpStream> {
        loop {
            // Sleep for 1/10 of a second to make sure that we aren't overusing resources
            thread::sleep(Duration::from_millis(100));

            // This should block until we encounter a client, if it doesn't block that's fine since we are sleeping.
            // Once we encounter a client, we want to try and open the TcpStream from the client so that we can communicate
            if let Ok(c) = TcpListener::bind(&format!("0.0.0.0:{}", crate::PORT)) {
                break Self::open_client(c);
            }
        }
    }

    /// Starts the server with a specified packet queue
    fn start_server(
        self,
        incoming_queue: TS_PacketQueue,
        outgoing_queue: TS_PacketQueue,
        cmd_rx: Receiver<ServerCommand>,
        event_tx: Sender<ServerEvent>,
        get_stream: impl Fn() -> anyhow::Result<TcpStream>
    ) -> anyhow::Result<()>
    {
        info!("Starting server!");

        let mut listener = get_stream()?;

        // Send the connected event to the receiver
        Self::send_event(&event_tx, ServerEvent::Connected)?;

        let mut is_paused = false;
        let mut should_kill = false;
        let mut can_kill = false;
        loop {
            // Server sleeps for 10ms
            if should_kill && can_kill {
                break;
            }

            thread::sleep(Duration::from_millis(10));

            // Try to get any incoming commands to know if it should perform an action or not
            match cmd_rx.try_recv() {
                // If we have a command then we need to execute it
                Ok(command) => match command {
                    ServerCommand::Pause => is_paused = true,
                    ServerCommand::Resume => is_paused = false,
                    ServerCommand::Kill => should_kill = true,
                },
                // If we are disconnected from the client, we need to abort because it means we no longer have a handle on the server
                Err(TryRecvError::Disconnected) => return Err(TryRecvError::Disconnected.into()),
                // Don't do anything if there was no command and we didn't disconnect
                _ => {}
            }

            if is_paused { continue; }

            // Handle sending packets
            if let Some(mut outgoing_queue) = outgoing_queue.try_write() {
                // Swap queue out so that we don't block any new packets coming in while we send the old ones out
                let mut queue = VecDeque::new();
                std::mem::swap(&mut queue, &mut *outgoing_queue);
                drop(outgoing_queue);

                // Send the packets and dip out if we fail
                if !queue.is_empty() {
                    Self::send_outgoing(queue, &mut listener)?;
                } else {
                    can_kill = true;
                }
            }
            
            match Self::get_incoming(&mut listener) {
                Ok(packet) => {
                    let mut queue = incoming_queue.write();
                    queue.push_back(packet);
                },
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {},
                Err(e) => return Err(e.into())
            }
        }

        info!("Server shutting down!");

        Ok(())
    }

    // I'm using a thread handle here because I can definitely tell that the switch is *not* suitable
    // for using an async runtime
    /// Starts the [Server]. Usually done by consoles to prevent an infinite boot sequence
    pub fn start_as_server(self) -> ServerInfo {
        // Create a packet queue and clone it so that we can use it in both the server thread and the server application
        let incoming_packet_queue = Self::create_queue();
        let thread_incoming_packet_queue = incoming_packet_queue.clone();

        let outgoing_packet_queue = Self::create_queue();
        let thread_outgoing_packet_queue = outgoing_packet_queue.clone();

        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();

        let join_handle = thread::spawn(move || {
            // We aren't returning an error here because I'd like to log it
            if let Err(e) = self.start_server(thread_incoming_packet_queue, thread_outgoing_packet_queue, cmd_rx, event_tx, Self::get_stream_as_server) {
                error!("Server exited prematurely! Error: {:?}", e);
            }
        });

        // Construct a ServerInfo struct for the user
        ServerInfo {
            thread_join_handle: Some(join_handle),
            incoming_packet_queue,
            outgoing_packet_queue,
            cmd_channel: cmd_tx,
            event_channel: event_rx
        }
    }

    /// Starts the [Server] as a client, same functionality but someone else is hosting.
    pub fn start_as_client(self, server_ip: String) -> ServerInfo {
        // Create a packet queue and clone it so that we can use it in both the server thread and the server application
        let incoming_packet_queue = Self::create_queue();
        let thread_incoming_packet_queue = incoming_packet_queue.clone();

        let outgoing_packet_queue = Self::create_queue();
        let thread_outgoing_packet_queue = outgoing_packet_queue.clone();

        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();

        let join_handle = thread::spawn(move || {
            // We aren't returning an error here because I'd like to log it
            if let Err(e) = self.start_server(
                thread_incoming_packet_queue, 
                thread_outgoing_packet_queue, 
                cmd_rx, 
                event_tx, 
                move || Self::get_stream_as_client(server_ip.as_str())
            )
            {
                error!("Server exited prematurely! Error: {:?}", e);
            }
        });

        // Construct a ServerInfo struct for the user
        ServerInfo {
            thread_join_handle: Some(join_handle),
            incoming_packet_queue,
            outgoing_packet_queue,
            cmd_channel: cmd_tx,
            event_channel: event_rx
        }
    }
}