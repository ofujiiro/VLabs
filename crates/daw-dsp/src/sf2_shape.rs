// SPDX-License-Identifier: GPL-3.0-or-later

//! A SoundFont with another note shape (decision 0031): the attack and
//! release of every sound of its instruments set to given times.
//!
//! The SoundFont player (`rustysynth`) reads a sound's envelope from the
//! file and offers no way to change it while playing, so a shaped
//! SoundFont is a copy of the file whose instrument zones say the new
//! times: each zone that plays a sample gets the two generators (replacing
//! its own, or added before its sample), and the presets' offsets to them
//! are cleared, so the times are exactly the ones asked for.

/// The generator numbers of the SoundFont 2 specification (section 8.1.2).
const ATTACK_VOL_ENV: u16 = 34;
const RELEASE_VOL_ENV: u16 = 38;
const SAMPLE_ID: u16 = 53;

/// The shortest time a SoundFont can say, in timecents (about a
/// millisecond), and the longest the specification allows for these two.
const MIN_TIMECENTS: i16 = -12_000;
const MAX_TIMECENTS: i16 = 8_000;

/// A time in seconds as SoundFont timecents (1200 × log2 of the seconds).
pub fn timecents(seconds: f32) -> i16 {
    if !seconds.is_finite() || seconds <= 0.001 {
        return MIN_TIMECENTS;
    }
    #[allow(
        clippy::cast_possible_truncation,
        reason = "clamped to the range of the generators"
    )]
    let value = (1200.0 * seconds.log2())
        .round()
        .clamp(f32::from(MIN_TIMECENTS), f32::from(MAX_TIMECENTS)) as i16;
    value
}

/// The bytes of a SoundFont whose instruments' attack and release (in
/// timecents) are the ones given; `None` keeps the file's own.
pub fn shaped(bytes: &[u8], attack: Option<i16>, release: Option<i16>) -> Result<Vec<u8>, String> {
    let wanted: Vec<(u16, i16)> = [(ATTACK_VOL_ENV, attack), (RELEASE_VOL_ENV, release)]
        .into_iter()
        .filter_map(|(oper, value)| value.map(|v| (oper, v)))
        .collect();
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"sfbk" {
        return Err("not a SoundFont".to_owned());
    }
    if wanted.is_empty() {
        return Ok(bytes.to_vec());
    }
    let mut out = Vec::with_capacity(bytes.len() + 4096);
    out.extend_from_slice(b"RIFF\0\0\0\0sfbk");
    let mut found = false;
    for (id, body) in chunks(&bytes[12..])? {
        if id == *b"LIST" && body.get(0..4) == Some(b"pdta") {
            let pdta = shaped_pdta(&body[4..], &wanted)?;
            write_chunk(&mut out, *b"LIST", &[b"pdta".as_slice(), &pdta].concat());
            found = true;
        } else {
            write_chunk(&mut out, id, body);
        }
    }
    if !found {
        return Err("the SoundFont has no preset data".to_owned());
    }
    let size = u32::try_from(out.len() - 8).map_err(|_| "too large".to_owned())?;
    out[4..8].copy_from_slice(&size.to_le_bytes());
    Ok(out)
}

/// A RIFF chunk: its identifier and contents.
type Chunk<'a> = ([u8; 4], &'a [u8]);

/// The sub-chunks of a RIFF list's body.
fn chunks(mut data: &[u8]) -> Result<Vec<Chunk<'_>>, String> {
    let mut found = Vec::new();
    while data.len() >= 8 {
        let id = [data[0], data[1], data[2], data[3]];
        let size = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
        let end = 8usize
            .checked_add(size)
            .filter(|&end| end <= data.len())
            .ok_or_else(|| "a damaged SoundFont (a chunk runs past its end)".to_owned())?;
        found.push((id, &data[8..end]));
        // Chunks are padded to an even size.
        data = &data[(end + (size & 1)).min(data.len())..];
    }
    Ok(found)
}

fn write_chunk(out: &mut Vec<u8>, id: [u8; 4], body: &[u8]) {
    out.extend_from_slice(&id);
    out.extend_from_slice(&u32::try_from(body.len()).unwrap_or(u32::MAX).to_le_bytes());
    out.extend_from_slice(body);
    if body.len() % 2 == 1 {
        out.push(0);
    }
}

/// A 4-byte record of a bag (generator and modulator indices) or a
/// generator (operator and amount).
fn records(body: &[u8]) -> Vec<[u16; 2]> {
    body.as_chunks::<4>()
        .0
        .iter()
        .map(|r| {
            [
                u16::from_le_bytes([r[0], r[1]]),
                u16::from_le_bytes([r[2], r[3]]),
            ]
        })
        .collect()
}

fn record_bytes(records: &[[u16; 2]]) -> Vec<u8> {
    records
        .iter()
        .flat_map(|[a, b]| a.to_le_bytes().into_iter().chain(b.to_le_bytes()))
        .collect()
}

/// The preset data with the wanted generators in every instrument zone
/// that plays a sample, and no preset offsets to them.
fn shaped_pdta(body: &[u8], wanted: &[(u16, i16)]) -> Result<Vec<u8>, String> {
    let parts = chunks(body)?;
    let part = |name: &[u8; 4]| {
        parts
            .iter()
            .find(|(id, _)| id == name)
            .map(|(_, body)| *body)
            .ok_or_else(|| {
                format!(
                    "the SoundFont's preset data has no {} part",
                    String::from_utf8_lossy(name)
                )
            })
    };
    // The presets' offsets to these generators are cleared.
    let mut pgen = records(part(b"pgen")?);
    for record in &mut pgen {
        if wanted.iter().any(|(oper, _)| *oper == record[0]) {
            record[1] = 0;
        }
    }
    // Every instrument zone that plays a sample says the wanted times.
    let mut ibag = records(part(b"ibag")?);
    let igen = records(part(b"igen")?);
    if ibag.is_empty() {
        return Err("the SoundFont has no instrument zones".to_owned());
    }
    let index = |bag: &[u16; 2]| usize::from(bag[0]).min(igen.len());
    let closing = index(&ibag[ibag.len() - 1]);
    let mut new_igen: Vec<[u16; 2]> = Vec::with_capacity(igen.len() + ibag.len() * wanted.len());
    let zones = ibag.len() - 1;
    for zone in 0..zones {
        let (from, to) = (
            index(&ibag[zone]),
            index(&ibag[zone + 1]).max(index(&ibag[zone])),
        );
        let mut gens: Vec<[u16; 2]> = igen[from..to].to_vec();
        let plays_sample = gens.last().is_some_and(|g| g[0] == SAMPLE_ID);
        for &(oper, value) in wanted {
            #[allow(
                clippy::cast_sign_loss,
                reason = "the amount's bits, as the file keeps them"
            )]
            let amount = value as u16;
            if let Some(existing) = gens.iter_mut().find(|g| g[0] == oper) {
                existing[1] = amount;
            } else if plays_sample {
                // Before the sample, which must stay the zone's last.
                let at = gens.len() - 1;
                gens.insert(at, [oper, amount]);
            }
        }
        ibag[zone][0] = u16::try_from(new_igen.len())
            .map_err(|_| "the SoundFont has too many generators to change".to_owned())?;
        new_igen.extend(gens);
    }
    // The closing zone, and the closing record after it.
    ibag[zones][0] = u16::try_from(new_igen.len())
        .map_err(|_| "the SoundFont has too many generators to change".to_owned())?;
    new_igen.extend_from_slice(&igen[closing..]);
    if new_igen.len() == usize::from(ibag[zones][0]) {
        new_igen.push([0, 0]);
    }

    let mut out = Vec::with_capacity(body.len() + new_igen.len() * 4);
    for (id, contents) in &parts {
        match id {
            b"pgen" => write_chunk(&mut out, *id, &record_bytes(&pgen)),
            b"ibag" => write_chunk(&mut out, *id, &record_bytes(&ibag)),
            b"igen" => write_chunk(&mut out, *id, &record_bytes(&new_igen)),
            _ => write_chunk(&mut out, *id, contents),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_become_timecents() {
        assert_eq!(timecents(1.0), 0);
        assert_eq!(timecents(2.0), 1200);
        assert_eq!(timecents(0.5), -1200);
        assert_eq!(timecents(0.0), MIN_TIMECENTS);
        assert_eq!(timecents(f32::NAN), MIN_TIMECENTS);
        assert_eq!(timecents(10_000.0), MAX_TIMECENTS);
    }

    #[test]
    fn what_is_not_a_soundfont_is_refused() {
        assert!(shaped(b"RIFF\0\0\0\0WAVE", Some(0), None).is_err());
        assert!(shaped(b"", Some(0), None).is_err());
    }
}
