//! Functions for handling renlib files.
use std::str;
use crate::errors::*;

use crate::board_logic::{BoardMarker, Stone, Point};
use crate::move_node::{MoveGraph, MoveIndex};

pub enum Version {
    V30,
    V34,
    _extended, // Reserve right to extend enum
}

#[derive(Debug)]
pub enum CommandVariant {
    Down,
    Right,
    OldComment,
    Mark,
    Comment,
    Start,
    NoMove,
    Extension,
    Mask,
}

impl CommandVariant {
    pub fn to_u8(&self) -> u8 {
        use self::CommandVariant::*;
        match *self {
            Down => 0x80,
            Right => 0x40,
            OldComment => 0x20,
            Mark => 0x10,
            Comment => 0x08,
            Start => 0x04,
            NoMove => 0x02,
            Extension => 0x01,
            Mask => 63// 0xFFFF3F,
        }
    }
}

#[derive(Debug)]
pub struct Command(pub u8);

impl Command {
    fn flag(&self, command: &CommandVariant) -> bool {
        let check = command.to_u8();
        self.0 & check == check
    }

    pub fn get_all(&self) -> Vec<CommandVariant> {
        use self::CommandVariant::*;
        let variants = vec![Down,Right,OldComment,Mark,Comment,Start,NoMove,Extension,Mask];
        variants.into_iter().filter(|variant| self.flag(variant)).collect()
    }

    pub fn is_down(&self) -> bool {
        self.flag(&CommandVariant::Down)
    }

    pub fn is_right(&self) -> bool {
        self.flag(&CommandVariant::Right)
    }
    
    pub fn is_old_comment(&self) -> bool {
        self.flag(&CommandVariant::OldComment)
    }

    pub fn is_mark(&self) -> bool {
        self.flag(&CommandVariant::Mark)
    }

    pub fn is_comment(&self) -> bool {
        self.flag(&CommandVariant::Comment)
    }

    pub fn is_start(&self) -> bool {
        self.flag(&CommandVariant::Start)
    }
    
    pub fn is_no_move(&self) -> bool {
        self.flag(&CommandVariant::NoMove)
    }

    pub fn is_extension(&self) -> bool {
        self.flag(&CommandVariant::Extension)
    }

    pub fn is_mask(&self) -> bool {
        self.flag(&CommandVariant::Mask)
    }
}

pub fn parse_lib(file_u8: Vec<u8>) -> Result<MoveGraph, ParseError> {
    
    let (header, file) = file_u8.split_at(20);
    match validate_lib(header)? {
        Version::V30 => {
            parse_v3x(file, Version::V30)
        }
        Version::V34 => {
            parse_v3x(file, Version::V34)
        }
        _ => unimplemented!(),
    }
}

pub fn validate_lib(header: &[u8]) -> Result<Version, ParseError> {
    match *header {
        [0xff, 0x52, 0x65, 0x6e, 0x4c, 0x69, 0x62, 0xff, majv, minv,
          0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff] => {
            match (majv, minv) {
                (3, 0) => {
                    Ok(Version::V30)
                }
                (3, 4) => {
                    Ok(Version::V34)
                }
                (majv, minv) =>Err(ParseError::VersionNotSupported{majv, minv}),
            }
         }
        _ => Err(ParseError::NotSupported),
    } 
}

pub fn byte_to_point(byte: &u8) -> Result<Point, ParseError> {
    Ok(Point::new(
        (match byte.checked_sub(1) {
            Some(value) => value,
            None => return Err(ParseError::Other("Underflowed position".to_string()))
        } & 0x0f) as u32,
        (byte >> 4) as u32
    ))
}

fn parse_v3x(file: &[u8], _version: Version) -> Result<MoveGraph, ParseError> {
    let mut graph = MoveGraph::new();
    let mut prev_index: Option<MoveIndex> = None;
    let mut cur_index: Option<MoveIndex> = None;
    let mut cur_root: Option<MoveIndex> = None;
    let mut iter = file.iter().peekable();
    if iter.peek().copied() == Some(&0x00) {
        // No move start, ignore.
        // TODO: Is this valid?
        //iter.next();
    }
    // It should just work to do this sequentially and use move_graph functions, let's try that
    while iter.peek().is_some() {
        let mut _cur_marker: Option<BoardMarker> = None;
        let byte = iter.next().unwrap();
        if iter.peek().is_none() && byte == &0x0a {
            // This is really wierd and shouldn't happen, will have to investigate
            break;
        }
        let command = Command(*iter.next().ok_or_else(|| ParseError::Other("Expected a command byte, got nothing".to_string()))?);
        let point = if let Ok(point) = byte_to_point(byte) {
            point 
        } else {
            tracing::debug!("Nope");
            Point::null()
        };
        tracing::info!("Point: {:?} Command: ({:x}) {:?} Previous Index: {:?}", point, command.0, command.get_all(), cur_index);
        let stone = if let Some(cur_index) =  cur_index {
            if graph.moves_to_root(cur_index) % 2 == 1 {
                Stone::Black
            } else {
                Stone::White
            }
        } else {
            Stone::Black
        };

        
        _cur_marker = Some(BoardMarker::new(point, stone));
        if command.is_comment() {
            tracing::info!("Parsing comment");
            // Move into functon?
            {
                let mut title = Vec::new();
            let mut comment = Vec::new();

            while iter.peek().ok_or_else(|| ParseError::Other("File ended while parsing title".to_string()))? != &&0x00 {
                title.push(*iter.next().unwrap())
            }
            while iter.peek().ok_or_else(|| ParseError::Other("File ended while parsing comment".to_string()))? != &&0x00 {
                comment.push(*iter.next().unwrap())
            }
            // Marker has to be something, consider wrapping entirety in if let.
            if let Some(m) = _cur_marker.as_mut() { m.set_comment(format!("Title: {}, Comment: {}",
                     str::from_utf8(title.as_slice()).unwrap_or("Failed to parse title!"),
                     str::from_utf8(comment.as_slice()).unwrap_or("Failed to parse comment!"))) }
            }
            iter.next(); // Skip the 0x00
        }
        if cur_index.is_none() {
            prev_index = cur_index;
            cur_index = Some(graph.new_root(_cur_marker.clone().unwrap()));
            cur_root = cur_index;
        } else if !(command.is_down() && command.is_right()){
            prev_index = cur_index;
            cur_index = Some(graph.add_move(cur_index.unwrap(), _cur_marker.clone().unwrap())); 
        }
        if command.is_right() && command.is_down() {
            //tracing::info!("Popped markeds");
            //graph.marked_for_branch.pop();
            prev_index = cur_index;
            // This branch leaf is alone, go down immidiatly
            cur_index = graph.down_to_branch(cur_index.unwrap());
            graph.add_move(cur_index.unwrap(), _cur_marker.unwrap());
        } else {
            if command.is_right() {
                prev_index = None;
                cur_index = graph.down_to_branch(cur_index.unwrap());
                tracing::info!("Branching down to, res: {:?}", cur_index);
            }
            if command.is_down() {
                tracing::info!("Marking {:?} as branch.", prev_index.unwrap_or_else(|| cur_root.unwrap()));
                graph.mark_for_branch(prev_index.unwrap_or_else(|| cur_root.unwrap()));
            }
        }

        if command.is_no_move() {
            if let Some(byte) = iter.next() {
                if byte != &0x00 {
                    panic!("Expected 0x00, got 0x{:x} while skiping for no-move",byte);
                }
            }
        }

    }
    Ok(graph)
}
