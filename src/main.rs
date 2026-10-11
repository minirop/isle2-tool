#![allow(unused)]
use byteorder::BigEndian;
use byteorder::LittleEndian;
use byteorder::ReadBytesExt;
use byteorder::WriteBytesExt;
use clap::Parser;
use clap::Subcommand;
use gif::DecodeOptions;
use gif::Decoder;
use gif::Encoder;
use gif::Frame;
use std::borrow::Cow;
use std::fs;
use std::fs::File;

#[derive(Debug, Parser)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    // Compress a file with LZ77.
    Compress { input: String, output: String },
    // Decompress a file.
    Decompress { input: String, output: String },
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Commands::Compress { input, output } => compress(&input, &output),
        Commands::Decompress { input, output } => decompress(&input, &output),
    }
}

fn compress(input: &str, output: &str) -> anyhow::Result<()> {
    let buffer = if input.ends_with(".gif") {
        let mut decoder = DecodeOptions::new();
        decoder.set_color_output(gif::ColorOutput::Indexed);
        let file = File::open(input)?;
        let mut decoder = decoder.read_info(file)?;

        let palette = decoder.palette()?;
        let mut palette_file = File::create(format!("{input}.pal"))?;
        for rgb in palette.chunks_exact(3) {
            let red = (((rgb[0] & 0b11111000) as u16) >> 3);
            let green = (((rgb[1] & 0b11111000) as u16) << 2);
            let blue = (((rgb[2] & 0b11111100) as u16) << 8);
            let color = red | green | blue;
            palette_file.write_u16::<LittleEndian>(color)?;
        }

        // Process only the first frame
        let Some(frame) = decoder.read_next_frame()? else {
            panic!("Missing frame");
        };

        frame.buffer.to_vec()
    } else {
        fs::read(input)?
    };

    let mut processed_bytes = 0;
    let mut num_flag_bits = 0;
    let mut dst_buffer = vec![];
    let mut last_flag_position = 0;
    let mut last_flag_input_position = 0;
    let mut flags = 0u8;

    while processed_bytes < buffer.len() {
        if num_flag_bits == 0 {
            last_flag_position = dst_buffer.len();
            dst_buffer.push(0x37u8);
            last_flag_input_position = processed_bytes;
        }

        if find_token_with_length(&buffer, processed_bytes, 3).is_some() {
            let mut token_size = 0;
            let mut token_delta = 0;

            for attempted_token_size in 3..19 {
                if let Some(attempted_token_delta) =
                    find_token_with_length(&buffer, processed_bytes, attempted_token_size)
                {
                    token_size = attempted_token_size;
                    token_delta = attempted_token_delta - 1;
                } else {
                    break;
                }
            }

            let token_size_m3 = (token_size - 3) as u8;
            let byte = (token_size_m3 << 4) | ((token_delta >> 8) as u8);
            dst_buffer.push(byte);
            dst_buffer.push((token_delta & 0xFF) as u8);
            processed_bytes += token_size;
            flags |= (0x80 >> num_flag_bits);
        } else {
            dst_buffer.push(buffer[processed_bytes]);
            processed_bytes += 1;
        }

        num_flag_bits += 1;

        if num_flag_bits == 8 || processed_bytes >= buffer.len() {
            num_flag_bits = 0;
            dst_buffer[last_flag_position] = flags;
            flags = 0;
        }
    }

    let size = buffer.len() as u16;

    dst_buffer.insert(0, 0x10);
    dst_buffer.insert(1, (size & 0xFF) as u8);
    dst_buffer.insert(2, (size >> 8) as u8);
    dst_buffer.insert(3, 0);

    while (dst_buffer.len() % 4) != 0 {
        dst_buffer.push(0);
    }

    fs::write(output, dst_buffer)?;

    Ok(())
}

fn find_token_with_length(buffer: &Vec<u8>, offset: usize, length: usize) -> Option<u16> {
    if offset + length > buffer.len() || offset < 8 {
        return None;
    }

    let mut starting_point = offset - 8;
    loop {
        let delta = starting_point.abs_diff(offset) as u16;
        if delta > 0xFFF {
            return None;
        }

        let mut matched = true;
        for i in 0..length {
            if buffer[starting_point + i] != buffer[offset + i] {
                matched = false;
                break;
            }
        }

        if matched {
            return Some(delta);
        }

        if starting_point.checked_sub(1).is_none() {
            break;
        }
        starting_point -= 1;
    }

    None
}

fn decompress(input: &str, output: &str) -> anyhow::Result<()> {
    let mut file = File::open(input)?;
    let header = file.read_u8()?;
    assert_eq!(header, 0x10);

    let size = file.read_u16::<LittleEndian>()? as usize;
    let _ = file.read_u8()?;

    let mut buffer = vec![];
    while buffer.len() < size {
        let flags = file.read_u8()?;
        let mut i = 0;
        while i < 8 && buffer.len() < size {
            let kind = (flags & (0x80 >> i)) != 0;
            if !kind {
                let value = file.read_u8()?;
                buffer.push(value);
            } else {
                let value = file.read_u16::<BigEndian>()?;
                let disp = value & 0x0FFF;
                let n = (value >> 12) as u8;
                for j in 0..(n + 3) {
                    let v = buffer[buffer.len() - disp as usize - 1];
                    buffer.push(v);
                }
            }

            i += 1;
        }
    }

    if fs::exists(format!("{input}.pal"))? {
        let palette = read_palette(format!("{input}.pal"))?;
        let mut image = File::create(output)?;
        let mut encoder = Encoder::new(&mut image, 240, 160, &palette)?;
        let mut frame = Frame::default();
        frame.width = 240;
        frame.height = 160;
        frame.buffer = Cow::Borrowed(&*buffer);
        encoder.write_frame(&frame).unwrap();
    } else {
        fs::write(output, buffer)?;
    }

    Ok(())
}

fn read_palette(buffer: String) -> anyhow::Result<Vec<u8>> {
    let mut output = vec![];

    let mut file = File::open(buffer)?;
    while let Ok(color) = file.read_u16::<LittleEndian>() {
        let red = (color << 3) & 0b11111000;
        let green = (color >> 2) & 0b11111000;
        let blue = (color >> 8) & 0b11111100;

        output.push(red as u8);
        output.push(green as u8);
        output.push(blue as u8);
    }

    Ok(output)
}
