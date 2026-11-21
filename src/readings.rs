use pinyin::ToPinyinMulti;
use std::borrow::Cow;
use unicode_normalization::UnicodeNormalization;
use unicode_script::{Script, UnicodeScript};

/// Apply Mandarin tones to the syllables indicated by CBDB's romanization.
pub fn tone_marked(name: &str, romanization: &str) -> Option<String> {
    // Parenthesized text disambiguates records and is not part of the name.
    let name = name.split(['(', '（']).next()?;
    let romanization = romanization.split(['(', '（']).next()?;
    let hint = normalize_hint(romanization.trim())?;
    let chars: Vec<_> = name
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '·' | '・' | '•' | '‧' | '-'))
        .collect();
    if chars.is_empty() || chars.iter().any(|c| c.script() != Script::Han) {
        return None;
    }

    let mut output = String::with_capacity(hint.len() * 2);
    align(&chars, hint.trim(), &mut output).then_some(output)
}

fn normalize_hint(source: &str) -> Option<String> {
    let mut hint = String::with_capacity(source.len());
    for character in source.nfd() {
        match character {
            c if c.is_ascii_alphabetic() => hint.push(c),
            c if c.is_whitespace() => {
                if !hint.ends_with([' ', '\'', '-', '·']) {
                    hint.push(' ');
                }
            }
            ':' | '\u{308}' if hint.ends_with(['u', 'U']) => {
                let umlaut = if hint.pop() == Some('U') { 'V' } else { 'v' };
                hint.push(umlaut);
            }
            '\u{300}' | '\u{301}' | '\u{304}' | '\u{30c}' | '\u{302}' => (),
            '\'' | '’' | 'ʼ' => hint.push('\''),
            '-' | '‐' | '‑' => hint.push('-'),
            '·' | '・' | '•' | '‧' => hint.push('·'),
            _ => return None,
        }
    }
    Some(hint)
}

fn align(chars: &[char], hint: &str, output: &mut String) -> bool {
    let Some((&character, remaining)) = chars.split_first() else {
        return hint.is_empty();
    };
    let next = hint.trim_start_matches([' ', '\'', '-', '·']);
    if next.is_empty() {
        return false;
    }
    let checkpoint = output.len();
    let separator = &hint[..hint.len() - next.len()];

    if checkpoint == 0
        && let Some((plain, marked)) = surname_tone(character)
        && let Some(rest) = strip_syllable(next, plain)
        && (rest.is_empty() || rest.starts_with(' '))
    {
        push_syllable(output, separator, next, marked);
        if align(remaining, rest, output) {
            return true;
        }
        output.truncate(checkpoint);
    }

    if checkpoint == 0
        && let Some((first, second, marked)) = compound_surname(chars)
        && let Some(rest) = strip_syllable(next, first)
        && let Some(rest) = strip_syllable(rest.trim_start_matches(' '), second)
    {
        push_syllable(output, separator, next, marked);
        let given = rest.trim_start_matches(' ');
        match given.as_bytes().first() {
            Some(letter) if letter.is_ascii_alphabetic() => {
                let given = format!("{}{}", (*letter as char).to_ascii_uppercase(), &given[1..]);
                output.push(' ');
                if align(&chars[2..], &given, output) {
                    return true;
                }
            }
            None if chars.len() == 2 => return true,
            _ => (),
        }
        output.truncate(checkpoint);
    }

    let Some(readings) = character.to_pinyin_multi() else {
        return false;
    };
    let mut tried_lengths = 0u16;
    for reading in readings {
        let plain = reading.plain();
        let plain = if plain.contains('ü') {
            Cow::Owned(plain.replace('ü', "v"))
        } else {
            Cow::Borrowed(plain)
        };
        let Some(rest) = strip_syllable(next, &plain) else {
            continue;
        };

        // Two readings of this length can only match the same source syllable.
        // In that case prefer the first pronunciation listed in the lexicon.
        let length = 1 << plain.len();
        if tried_lengths & length != 0 {
            continue;
        }
        tried_lengths |= length;
        push_syllable(output, separator, next, reading.with_tone());
        if align(remaining, rest, output) {
            return true;
        }
        output.truncate(checkpoint);
    }
    false
}

fn strip_syllable<'a>(hint: &'a str, plain: &str) -> Option<&'a str> {
    let prefix = hint.get(..plain.len())?;
    prefix
        .eq_ignore_ascii_case(plain)
        .then(|| &hint[plain.len()..])
}

fn push_syllable(output: &mut String, separator: &str, hint: &str, marked: &str) {
    if separator.is_empty()
        && !output.is_empty()
        && hint.starts_with(['a', 'e', 'o', 'A', 'E', 'O'])
    {
        output.push('\'');
    }
    output.push_str(separator);
    if hint.as_bytes()[0].is_ascii_uppercase() {
        let mut letters = marked.chars();
        output.extend(letters.next().unwrap().to_uppercase());
        output.push_str(letters.as_str());
    } else {
        output.push_str(marked);
    }
}

fn compound_surname(chars: &[char]) -> Option<(&'static str, &'static str, &'static str)> {
    // See GB/T 28039-2011 and the Ministry of Education's Mandarin dictionaries.
    match chars {
        ['万' | '萬', '俟', ..] => Some(("mo", "qi", "mòqí")),
        ['尉', '遲' | '迟', ..] => Some(("yu", "chi", "yùchí")),
        ['長' | '长', '孫' | '孙', ..] => Some(("zhang", "sun", "zhǎngsūn")),
        ['諸' | '诸', '葛', ..] => Some(("zhu", "ge", "zhūgě")),
        _ => None,
    }
}

fn surname_tone(character: char) -> Option<(&'static str, &'static str)> {
    match character {
        '任' => Some(("ren", "rén")),
        '華' | '华' => Some(("hua", "huà")),
        '紀' | '纪' => Some(("ji", "jǐ")),
        '燕' => Some(("yan", "yān")),
        '曲' => Some(("qu", "qū")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::tone_marked;

    #[test]
    fn marks_canonical_and_alias_readings() {
        for (name, source, marked) in [
            ("曹操", "Cao Cao", "Cáo Cāo"),
            ("孟德", "Mengde", "Mèngdé"),
            ("王安石", "Wang Anshi", "Wáng Ānshí"),
            ("王荊公", "Wang Jinggong", "Wáng Jīnggōng"),
            ("蘇軾", "Su Shi", "Sū Shì"),
            ("呂布", "Lü Bu", "Lǚ Bù"),
            ("吕布", "Lu: Bu", "Lǚ Bù"),
            ("張氏（趙禎妃）", "Zhang Shi (zhaozhenfei)", "Zhāng Shì"),
            ("陳延安", "Chen Yanan", "Chén Yán'ān"),
            ("長孫無忌", "Zhangsun Wuji", "Zhǎngsūn Wújì"),
            ("樂毅", "Yue Yi", "Yuè Yì"),
            ("任伯雨", "Ren Boyu", "Rén Bóyǔ"),
            ("紀昀", "Ji Yun", "Jǐ Yún"),
            ("曲環", "Qu Huan", "Qū Huán"),
            ("燕肅", "Yan Su", "Yān Sù"),
            ("華佗", "Hua Tuo", "Huà Tuó"),
            ("万俟卨", "Moqi Xie", "Mòqí Xiè"),
            ("諸葛亮", "Zhuge Liang", "Zhūgě Liàng"),
            ("诸葛亮", "Zhu Geliang", "Zhūgě Liàng"),
        ] {
            assert_eq!(tone_marked(name, source).as_deref(), Some(marked), "{name}");
        }
    }

    #[test]
    fn omits_readings_when_the_source_cannot_be_aligned() {
        assert_eq!(tone_marked("曹操", ""), None);
        assert_eq!(tone_marked("曹操", "Cao"), None);
        assert_eq!(tone_marked("曹操", "Cao Tai"), None);
        assert_eq!(tone_marked("曹X", "Cao Xi"), None);
    }
}
