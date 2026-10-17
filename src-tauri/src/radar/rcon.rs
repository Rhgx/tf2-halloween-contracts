//! Source RCON client for TF2's own console (enabled with -usercon), and `getpos` parsing.
//!
//! `getpos` is a client-side command with no cheat flag. Plain `getpos` reports the camera position and the
//! real view angles; `getpos 2` reports the feet position (printed as setpos_exact).

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

const SERVERDATA_AUTH: i32 = 3;
const SERVERDATA_EXECCOMMAND: i32 = 2;
const SERVERDATA_AUTH_RESPONSE: i32 = 2;
const SERVERDATA_RESPONSE_VALUE: i32 = 0;

pub struct Rcon {
    stream: TcpStream,
    next_id: i32,
}

impl Rcon {
    pub fn connect(port: u16, password: &str) -> io::Result<Rcon> {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let stream = TcpStream::connect_timeout(&addr, Duration::from_millis(500))?;
        stream.set_read_timeout(Some(Duration::from_millis(1500)))?;
        stream.set_nodelay(true)?;
        let mut rcon = Rcon { stream, next_id: 1 };
        let id = rcon.send(SERVERDATA_AUTH, password)?;
        loop {
            let (reply_id, kind, _) = rcon.read_packet()?;
            if kind == SERVERDATA_AUTH_RESPONSE {
                return match reply_id {
                    -1 => Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "wrong rcon password",
                    )),
                    r if r == id => Ok(rcon),
                    _ => continue,
                };
            }
        }
    }

    /// Runs a console command and returns the console output TF2 sent back (may be empty).
    pub fn exec(&mut self, command: &str) -> io::Result<String> {
        let id = self.send(SERVERDATA_EXECCOMMAND, command)?;
        loop {
            let (reply_id, kind, body) = self.read_packet()?;
            if reply_id == id && kind == SERVERDATA_RESPONSE_VALUE {
                return Ok(body);
            }
        }
    }

    /// Runs a console command without waiting; the next `exec` skips its reply.
    pub fn fire(&mut self, command: &str) -> io::Result<()> {
        self.send(SERVERDATA_EXECCOMMAND, command).map(|_| ())
    }

    fn send(&mut self, kind: i32, body: &str) -> io::Result<i32> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        let mut packet = Vec::with_capacity(body.len() + 14);
        packet.extend_from_slice(&(body.len() as i32 + 10).to_le_bytes());
        packet.extend_from_slice(&id.to_le_bytes());
        packet.extend_from_slice(&kind.to_le_bytes());
        packet.extend_from_slice(body.as_bytes());
        packet.extend_from_slice(&[0, 0]);
        self.stream.write_all(&packet)?;
        Ok(id)
    }

    fn read_packet(&mut self) -> io::Result<(i32, i32, String)> {
        let mut size = [0u8; 4];
        self.stream.read_exact(&mut size)?;
        let size = i32::from_le_bytes(size);
        if !(10..=1 << 20).contains(&size) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "bad rcon packet size",
            ));
        }
        let mut data = vec![0u8; size as usize];
        self.stream.read_exact(&mut data)?;
        let id = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let kind = i32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        let body = &data[8..];
        let end = body.iter().position(|&b| b == 0).unwrap_or(body.len());
        Ok((id, kind, String::from_utf8_lossy(&body[..end]).into_owned()))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pose {
    /// Camera position (eyes).
    pub eye: [f32; 3],
    pub pitch: f32,
    pub yaw: f32,
    /// Feet position.
    pub feet: [f32; 3],
}

/// The newest complete pose in console output containing `getpos` and `getpos 2` results.
pub fn parse_pose(text: &str) -> Option<Pose> {
    let numbers = |s: &str| -> Option<[f32; 3]> {
        let mut it = s.split_whitespace().map(|v| v.parse::<f32>().ok());
        Some([it.next()??, it.next()??, it.next()??])
    };
    let (mut eye, mut angles, mut feet) = (None, None, None);
    for part in text.split(['\n', ';']) {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix("setpos_exact ") {
            feet = numbers(rest).or(feet);
        } else if let Some(rest) = part.strip_prefix("setpos ") {
            eye = numbers(rest).or(eye);
        } else if let Some(rest) = part.strip_prefix("setang ") {
            angles = numbers(rest).or(angles);
        }
    }
    let (eye, angles) = (eye?, angles?);
    Some(Pose {
        eye,
        pitch: angles[0],
        yaw: angles[1],
        feet: feet.unwrap_or([eye[0], eye[1], eye[2] - 68.0]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_getpos_and_getpos_2() {
        let text = "setpos 250.000000 -600.000000 -126.968750;setang 3.000000 90.000000 0.000000\n\
                    setpos_exact 250.000000 -600.000000 -194.968750;setang_exact 0.000000 90.000000 0.000000\n";
        let pose = parse_pose(text).unwrap();
        assert_eq!(pose.eye, [250.0, -600.0, -126.96875]);
        assert_eq!((pose.pitch, pose.yaw), (3.0, 90.0));
        assert_eq!(pose.feet, [250.0, -600.0, -194.96875]);
    }

    #[test]
    fn takes_the_newest_pose_and_ignores_noise() {
        let text = "setpos 1 2 3;setang 0 10 0\nsome other line\nsetpos 4 5 6;setang 0 20 0\n";
        let pose = parse_pose(text).unwrap();
        assert_eq!((pose.eye, pose.yaw), ([4.0, 5.0, 6.0], 20.0));
        assert!(parse_pose("nothing here").is_none());
    }
}
