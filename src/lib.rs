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
        input
            .iter_mut()
            .next()
            .filter(|ch| *ch == 'l')?;

        let result: Vec<_> = input
            .decode_list_iter_mut()
            .collect();

        input.iter_mut().next().filter(|ch| *ch == 'e')?;

        Some((serde_json::Value::Array(result.into()), input))
    }
}

struct DictDecoder;

impl Decoder for DictDecoder {
    fn try_decode(&self, mut input: BencodedDecodeInput) -> Option<(serde_json::Value, BencodedDecodeInput)> {
        input
            .iter_mut()
            .next()
            .filter(|ch| *ch == 'd')?;

        let result: serde_json::Map<_, _> = input
            .decode_dict_iter_mut()
            .collect();

        input
            .iter_mut()
            .next()
            .filter(|ch| *ch == 'e')?;

        Some((serde_json::Value::Object(result.into()), input))
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

struct BencodedDecodeListIterMut<'a> {
    input: &'a mut BencodedDecodeInput
}

struct BencodedDecodeDictIterMut<'a> {
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
            Some('d') => Box::new(DictDecoder),
            _ => Box::new(FailureDecoder)
        }
    }

    fn decode_list_iter_mut(&mut self) -> BencodedDecodeListIterMut {
        BencodedDecodeListIterMut { input: self }
    }

    fn decode_dict_iter_mut(&mut self) -> BencodedDecodeDictIterMut {
        BencodedDecodeDictIterMut { input: self }
    }
}

impl<'a> Iterator for BencodedDecodeListIterMut<'a> {
    type Item = serde_json::Value;

    fn next(&mut self) -> Option<Self::Item> {
        let decoder = self.input.next_decoder();
        let (decoded_value, rest) = decoder.run_decoder(self.input.clone());
        self.input.index = rest.index;
        decoded_value
    }
}

impl<'a> Iterator for BencodedDecodeDictIterMut<'a> {
    type Item = (String, serde_json::Value);

    fn next(&mut self) -> Option<Self::Item> {
        let (key, rest) = StringDecoder.run_decoder(self.input.clone());
        self.input.index = rest.index;
        let key = match key {
            Some(serde_json::Value::String(key)) => key,
            _ => None?
        };

        let decoder = self.input.next_decoder();
        let (decoded_value, rest) = decoder.run_decoder(self.input.clone());
        self.input.index = rest.index;
        Some((key, decoded_value?))
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

#[cfg(test)]
mod test {
    use crate::decode_bencoded_value;

    #[test]
    fn test_string() {
        assert_eq!(
            decode_bencoded_value("4:spam".into()),
            serde_json::json!("spam")
        );
        assert_eq!(
            decode_bencoded_value("0:".into()),
            serde_json::json!("")
        );
    }

    #[test]
    fn test_integer() {
        assert_eq!(
            decode_bencoded_value("i3e".into()),
            serde_json::json!(3)
        );
        assert_eq!(
            decode_bencoded_value("i-3e".into()),
            serde_json::json!(-3)
        );
    }

    #[test]
    fn test_list() {
        assert_eq!(
            decode_bencoded_value("l4:spam4:eggse".into()),
            serde_json::json!(["spam", "eggs"])
        );
        assert_eq!(
            decode_bencoded_value("le".into()),
            serde_json::json!([])
        );
        assert_eq!(
            decode_bencoded_value("li32elei2e1:se".into()),
            serde_json::json!([32, [], 2, "s"])
        );
    }

    #[test]
    fn test_dict() {
        assert_eq!(
            decode_bencoded_value("d3:cow3:moo4:spam4:eggse".into()),
            serde_json::json!({
                "cow": "moo",
                "spam": "eggs"
            })
        );
        assert_eq!(
            decode_bencoded_value("d4:spaml1:a1:bee".into()),
            serde_json::json!({"spam": ["a","b"]})
        );
        assert_eq!(
            decode_bencoded_value("d9:publisher3:bob17:publisher-webpage15:www.example.com18:publisher.location4:homee".into()),
            serde_json::json!({"publisher": "bob", "publisher-webpage": "www.example.com", "publisher.location": "home"})
        );
        assert_eq!(
            decode_bencoded_value("de".into()),
            serde_json::json!({})
        );
    }
}