use serde_json;
use std::env;
use std::rc::Rc;

// Available if you need it!
// use serde_bencode

type ParseResult<T> = (Option<T>, BencodedDecodeInput);

trait Decoder {
    fn try_decode(&self, input: BencodedDecodeInput) -> Option<(serde_json::Value, BencodedDecodeInput)>;

    fn run_decoder(&self, input: BencodedDecodeInput) -> ParseResult<serde_json::Value> {
        match self.try_decode(input.clone()) {
            Some((val, rest)) => (Some(val), rest),
            _ => (None, input)
        }
    }
}

// macro_rules! try_parse {
//     ($expr:expr, $block:block) => {{
//         match (|| -> Option<(serde_json::Value, &str)> { $block })() {
//             Some((val, rest)) => (Some(val), rest),
//             _ => (None, $expr)
//         }
//     }}
// }

struct StringDecoder;

impl Decoder for StringDecoder {
    fn try_decode(&self, mut input: BencodedDecodeInput) -> Option<(serde_json::Value, BencodedDecodeInput)> {
        let len = input
            .iter_mut()
            .take_while(|ch| *ch != ':')
            .fold(Some(0), |acc, num| {
                let acc = acc?;
                let num = num.to_digit(10)? as usize;
                Some(10*acc + num)
            })?;

        let val = input
            .iter_mut()
            .take(len)
            .collect::<String>();
        Some((serde_json::Value::String(val), input))
    }
}

struct IntegerDecoder;

impl Decoder for IntegerDecoder {
    fn try_decode(&self, mut input: BencodedDecodeInput) -> Option<(serde_json::Value, BencodedDecodeInput)> {
        let mut digits = input
            .iter_mut()
            .skip(1)
            .take_while(|&ch| ch != 'e');

        let first_char = digits.next()?;

        let (is_neg, init) = first_char
            .to_digit(10)
            .map(|val| (false, val as i64))
            .or_else(|| {
                let ch = digits.next()?;
                let digit = ch.to_digit(10)? as i64;
                Some((true, digit))
            })?;

        let mut num = digits
            .fold(Some(init), |acc, val| {
                let acc = acc?;
                let val = val.to_digit(10)? as i64;
                Some(10*acc + val)
            })?;
        
        if is_neg {
            num = -num;
        }

        Some((serde_json::Value::Number(num.into()), input))
    }
}

struct FailureDecoder;

impl Decoder for FailureDecoder {
    fn try_decode(&self, _: BencodedDecodeInput) -> Option<(serde_json::Value, BencodedDecodeInput)> {
        None
    }
}

struct ListDecoder;

impl Decoder for ListDecoder {
    fn try_decode(&self, mut input: BencodedDecodeInput) -> Option<(serde_json::Value, BencodedDecodeInput)> {
        let start_ch = input.iter_mut().next()?;
        (start_ch == 'l').then_some(())?;
        let result_iter = input
            .decode_iter_mut();

        let results: Vec<_> = result_iter
            .collect();

        Some((serde_json::Value::Array(results.into()), input))
    }
}

#[derive(Clone)]
struct BencodedDecodeInput {
    index: usize,
    data: Rc<Vec<char>>
}

impl std::fmt::Debug for BencodedDecodeInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{index: {:?}, data: {:?}}}", self.index, self.data.iter().cloned().collect::<String>())
    }
}

impl std::fmt::Display for BencodedDecodeInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{index: {}, data: {}}}", self.index, self.data.iter().cloned().collect::<String>())
    }
}

struct BencodedDecodeInputIterMut<'a> {
    input: &'a mut BencodedDecodeInput
}

impl<'a> Iterator for BencodedDecodeInputIterMut<'a> {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        self
            .input
            .data
            .get(self.input.index)
            .map(|&ch| {
                self.input.index += 1;
                ch
            })
    }
}

struct BencodedDecodeInputIter {
    input: BencodedDecodeInput
}

impl Iterator for BencodedDecodeInputIter {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        self
            .input
            .data
            .get(self.input.index)
            .map(|&ch| {
                self.input.index += 1;
                ch
            })
    }
}

struct BencodedDecodeIterMut<'a> {
    input: &'a mut BencodedDecodeInput
}

impl BencodedDecodeInput {
    fn new(data: Vec<char>) -> Self {
        Self { index: 0, data: Rc::new(data) }
    }

    fn iter(&self) -> BencodedDecodeInputIter {
        BencodedDecodeInputIter { input: self.clone() }
    }

    fn iter_mut(&mut self) -> BencodedDecodeInputIterMut {
        BencodedDecodeInputIterMut { input: self }
    }

    fn next_decoder(&self) -> Box<dyn Decoder> {
        match self.iter().next() {
            Some('0'..='9') => Box::new(StringDecoder),
            Some('i') => Box::new(IntegerDecoder),
            Some('l') => Box::new(ListDecoder),
            _ => Box::new(FailureDecoder)
        }
    }

    fn decode_iter_mut(&mut self) -> BencodedDecodeIterMut {
        BencodedDecodeIterMut { input: self }
    }
}

impl<'a> Iterator for BencodedDecodeIterMut<'a> {
    type Item = serde_json::Value;

    fn next(&mut self) -> Option<Self::Item> {
        let decoder = self.input.next_decoder();
        let (decoded_value, rest) = decoder.run_decoder(self.input.clone());
        self.input.index = rest.index;
        decoded_value
    }
}

#[allow(dead_code)]
fn decode_bencoded_value(encoded_value: String) -> serde_json::Value {
    let data = encoded_value
        .chars()
        .collect();
    let input = BencodedDecodeInput::new(data);
    let (result, _) = input
        .next_decoder()
        .run_decoder(input.clone());

    match result {
        Some(result) => result,
        None => panic!("Parsing failed for {:?}", input)
    }
}

// Usage: your_bittorrent.sh decode "<encoded_value>"
fn main() {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap();

    if command == "decode" {
        let encoded_value = args.next().unwrap();
        let decoded_value = decode_bencoded_value(encoded_value);
        println!("{}", decoded_value.to_string());
    } else {
        println!("unknown command: {}", command)
    }
}
