use error::MainError;
use std::io::Read;
use std::str;
use tudelft_dsmr_output_generator::voltage_over_time::{
    create_voltage_over_time_graph, VoltageData,
};
use tudelft_dsmr_output_generator::Graphs;

/// Contains `MainError`, and code to convert `PlotError` and `io::Error` into a `MainError`
mod error;

fn get_month_as_uint(date: &str) -> u8 {
    match &date[..date.len() - 1] {
        "Ja" => 1,
        "Fe" => 2,
        "Ap" => 4,
        "Au" => 8,
        "Se" => 9,
        "Oc" => 10,
        "No" => 11,
        "De" => 12,
        "Ma" => {
            if &date[(date.len() - 1)..] == "y" {
                3
            } else {
                5
            }
        }
        "Ju" => {
            if &date[(date.len() - 1)..] == "n" {
                6
            } else {
                7
            }
        }
        _ => 0, // should never reach here;
    }
}

type DsmrV10 = Vec<TelegramV10>;

#[derive(Debug)]
struct TelegramV10 {
    date: String,
    event_log: EventLog,
    information: Electricity,
}

#[derive(Clone, Debug)]
enum Phase {
    Phase1 = 1,
    Phase2,
    Phase3,
}

#[derive(Clone, Debug)]
struct Electricity {
    power_phase: Phase,
    current_phase: Phase,
    voltage_phase: Phase,

    power: f64,
    voltage: f64,
    current: f64,
    total_consumed: f64,
    total_produced: f64,
}

#[derive(Copy, Clone, Debug)]
enum Severity {
    Low,
    High,
}

#[derive(Clone, Debug)]
struct EventLog {
    ids: Vec<u32>,
    severities: Vec<Severity>, // should be String?
    dates: Vec<i64>,
    messages: Vec<String>,
}

#[derive(Debug)]
pub enum ParseError {
    UnknownTelegramVersion,
    NoDate,
    DuplicateFieldId,
    MissingElectricity,
    ChildTelegramNotSupported,
}

/// Parse v10 of DSMR spec
fn parse_v10(input: &str) -> Result<DsmrV10, ParseError> {
    let mut lines = input.lines();
    lines.next();

    let mut dsmr = DsmrV10::new();
    let mut electricity = Electricity {
        current_phase: Phase::Phase1,
        voltage_phase: Phase::Phase1,
        power_phase: Phase::Phase1,

        power: 0.0,
        voltage: 0.0,
        current: 0.0,
        total_consumed: 0.0,
        total_produced: 0.0,
    };

    // TODO: i64 should be UnixTimeStamp for clarity
    let mut telegram_date = String::new();
    let mut dates = Vec::new();
    let mut messages = Vec::new();
    let mut severities = Vec::new();

    let mut seen_info_type = false;
    let mut has_electricity = false;
    let mut has_telegram_date = false;
    for line in lines {
        if line.is_empty() {
            continue;
        }

        let bytes = line.as_bytes();
        match bytes[0] {
            b'1' => {
                // parse telegram header

                if bytes[2] == b'1' && bytes[4] != b'0' {
                    return Err(ParseError::ChildTelegramNotSupported);
                }
                if bytes[2] == b'2' {
                    // telegram end
                    if !has_electricity {
                        return Err(ParseError::MissingElectricity);
                    }
                    if !has_telegram_date {
                        return Err(ParseError::NoDate);
                    }

                    // sort by the event id
                    dates.sort_unstable_by(|ent_a: &(u32, i64), ent_b: &(u32, i64)| {
                        ent_a.0.partial_cmp(&ent_b.0).unwrap()
                    });
                    severities.sort_unstable_by(
                        |ent_a: &(u32, Severity), ent_b: &(u32, Severity)| {
                            ent_a.0.partial_cmp(&ent_b.0).unwrap()
                        },
                    );
                    messages.sort_unstable_by(|ent_a: &(u32, String), ent_b: &(u32, String)| {
                        ent_a.0.partial_cmp(&ent_b.0).unwrap()
                    });

                    // ensure the whole thing correlate
                    assert!(dates.len() == severities.len());
                    assert!(dates.len() == messages.len());

                    let mut event_log = EventLog {
                        ids: Vec::new(),
                        dates: Vec::new(),
                        messages: Vec::new(),
                        severities: Vec::new(),
                    };

                    for i in 0..dates.len() {
                        let ent_date = dates[i];
                        let ent_sev = severities[i];
                        let ent_msg = messages[i].clone();

                        // ensure the IDs are the same
                        assert!(ent_date.0 == ent_msg.0);
                        assert!(ent_date.0 == ent_sev.0);

                        let id = ent_date.0;

                        event_log.ids.push(id);
                        event_log.dates.push(ent_date.1);
                        event_log.messages.push(ent_msg.1);
                        event_log.severities.push(ent_sev.1);
                    }

                    // push it to list
                    let telegram_v10 = TelegramV10 {
                        date: telegram_date.clone(),
                        event_log: event_log.clone(),
                        information: electricity.clone(),
                    };
                    // get the compiler to shut up
                    _ = telegram_v10.date;
                    _ = telegram_v10.event_log;
                    _ = telegram_v10.information;
                    dsmr.push(telegram_v10);
                }

                // new telegram
                seen_info_type = false;
                has_electricity = false;
                has_telegram_date = false;

                telegram_date.clear();
                dates.clear();
                severities.clear();
                messages.clear();

                electricity.power = 0.0;
                electricity.voltage = 0.0;
                electricity.current = 0.0;
                electricity.total_consumed = 0.0;
                electricity.total_produced = 0.0;
            }
            b'2' => {
                let idx = line.rfind(')').unwrap();
                telegram_date = String::from_utf8_lossy(&bytes[5..idx]).to_string();
                has_telegram_date = true;
            }
            b'3' => {
                // parse eventlog

                // in 3.x.n; x is discriminant, n is event id
                let discriminant = bytes[2] as char;
                let paren = line.rfind(')').unwrap();
                let val = &bytes[7..paren];

                let event_id = bytes[4] as char;
                let event_id = event_id.to_digit(10).unwrap();
                match discriminant {
                    '1' => severities.push((
                        event_id,
                        if matches!(bytes[7] as char, 'H') {
                            Severity::High
                        } else {
                            Severity::Low
                        },
                    )),
                    '2' => messages.push((event_id, String::from_utf8_lossy(&val).to_string())),
                    '3' => {
                        // parse date

                        // <yy-mmm-dd hh:mm:ss>
                        let date = String::from_utf8_lossy(&val[..val.len() - 4]).to_string();

                        let dts = val[val.len() - 2] as char;
                        let yy = 2000 /* account for this century */ + (&date[0..2]).parse::<u16>().unwrap();
                        let dd: u8 = (&date[7..9]).parse().unwrap();
                        let hh: u8 = (&date[10..12]).parse().unwrap();
                        let mm: u8 = (&date[13..15]).parse().unwrap();
                        let ss: u8 = (&date[16..18]).parse().unwrap();
                        let mmm: u8 = get_month_as_uint(&date[3..6]);

                        dates.push((
                            event_id,
                            tudelft_dsmr_output_generator::date_to_timestamp(
                                yy,
                                mmm,
                                dd,
                                hh,
                                mm,
                                ss,
                                dts == 'S',
                            )
                            .unwrap(),
                        ))
                    }
                    _ => unreachable!(),
                }
            }
            b'4' => {
                // parse informtion type

                if seen_info_type {
                    return Err(ParseError::DuplicateFieldId);
                }

                seen_info_type = true;
            }
            b'7' => {
                // parse electricity
                if !seen_info_type {
                    return Err(ParseError::MissingElectricity);
                }
                has_electricity = true;

                let phase = match bytes[4] as char {
                    '1' => Phase::Phase1,
                    '2' => Phase::Phase2,
                    '3' => Phase::Phase3,
                    _ => unreachable!(),
                };
                let discriminant = bytes[2] as char;
                let star = line.find('*').unwrap();
                let val = str::from_utf8(&bytes[7..star]).unwrap();
                let val_f64 = val.parse::<f64>().unwrap();

                if '1' <= discriminant && discriminant <= '4' {
                    match discriminant {
                        '1' => {
                            electricity.voltage_phase = phase;
                            electricity.voltage = val_f64.max(electricity.voltage);
                        }
                        '2' => {
                            electricity.current = val_f64.max(electricity.current);
                            electricity.current_phase = phase;
                        }
                        '3' => {
                            electricity.power += val_f64;
                            electricity.power_phase = phase;
                        }
                        '4' => {
                            if bytes[4] == b'1' {
                                electricity.total_consumed += val_f64;
                            } else {
                                electricity.total_produced += val_f64;
                            }
                        }
                        _ => unreachable!(),
                    }
                } else {
                }
            }
            _ => {}
        }
    }
    Ok(dsmr)
}

fn parse(_input: &str) -> Result<DsmrV10, ParseError> {
    // Note that you can use this function:
    // tudelft_dsmr_output_generator::date_to_timestamp(4);
    let version = &_input[1..4];
    if version == "v10" {
        return parse_v10(_input);
    } else {
        return Err(ParseError::UnknownTelegramVersion);
    }
}

#[test]
pub fn fail_on_duplicate_info_type() {
    let input = std::fs::read_to_string("examples/bad/duplicate_info.dsmr").unwrap();
    assert!(parse(&input).is_err());
}
#[test]
pub fn fail_on_missing_electricity() {
    let input = std::fs::read_to_string("examples/bad/missing_electricity.dsmr").unwrap();
    assert!(parse(&input).is_err());
}
#[test]
pub fn fail_on_missing_date() {
    let input = std::fs::read_to_string("examples/bad/no_date.dsmr").unwrap();
    assert!(parse(&input).is_err());
}

#[test]
pub fn fail_on_v12() {
    for i in 0..4 {
        let fp_rec = std::fmt::format(format_args!(
            "examples/good_sequences/should_parse_{i}_recursive.dsmr"
        ));

        let input = std::fs::read_to_string(fp_rec).unwrap();
        assert!(parse(&input).is_err());
    }
}

#[test]
pub fn can_parse_multiple_telegrams() {
    let input = std::fs::read_to_string("examples/good/two_packets.dsmr").unwrap();
    let maybe_dsmr = parse(&input);
    println!("{maybe_dsmr:#?}");
    assert!(maybe_dsmr.is_ok());
    assert_eq!(maybe_dsmr.unwrap().len(), 2);

    for i in 0..4 {
        let fp = std::fmt::format(format_args!(
            "examples/good_sequences/should_parse_{i}.dsmr"
        ));
        let input = std::fs::read_to_string(fp).unwrap();
        let count = input.matches("(END)").count();
        let maybe_dsmr = parse(&input);
        assert!(maybe_dsmr.is_ok());
        assert_eq!(maybe_dsmr.unwrap().len(), count);
    }
}

/// Reads the DSMR file from the terminal.
/// You do not need to change this nor understand this.
/// You can use
/// ```
/// cargo run < examples/good/simple_gas.dsmr
/// ```
/// to quickly test an example dsmr file with your submission.
/// We also use this at the end to assist with grading your submission!
fn read_from_stdin() -> Result<String, MainError> {
    let mut input = Vec::new();
    let stdin = std::io::stdin();
    let mut handle = stdin.lock();
    handle.read_to_end(&mut input)?;
    Ok(String::from_utf8_lossy(&input).to_string())
}

fn main() -> Result<(), MainError> {
    let input = read_from_stdin()?;

    let parsed = parse(&input).unwrap_or_else(|_| std::process::exit(42));

    println!("parsed: {parsed:#?}");

    let mut result = Graphs::new()?;

    result.add_graph(create_voltage_over_time_graph(vec![
        VoltageData {
            phase_1: 100.0,
            phase_2: 200.0,
            phase_3: 300.0,
            timestamp: 100,
        },
        VoltageData {
            phase_1: 200.0,
            phase_2: 300.0,
            phase_3: 250.0,
            timestamp: 10000,
        },
    ]))?;
    result.generate()?;

    Ok(())
}
