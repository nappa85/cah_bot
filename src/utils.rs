const TO_BE_ESCAPED: &[char] = &[
    '_', '*', '[', ']', '(', ')', '~', '`', '>', '#', '+', '-', '=', '|', '{', '}', '.', '!',
];

pub fn escape_markdown(text: impl AsRef<str>) -> String {
    text.as_ref()
        .chars()
        .flat_map(|c| {
            if TO_BE_ESCAPED.contains(&c) {
                [Some('\\'), Some(c)]
            } else {
                [None, Some(c)]
            }
        })
        .flatten()
        .collect()
}

pub fn unescape_markdown(text: impl AsRef<str>) -> String {
    let mut iter = text.as_ref().chars().peekable();
    let mut out = String::with_capacity(text.as_ref().len());
    while let Some(c) = iter.next() {
        if c != '\\' || !iter.peek().is_some_and(|c| TO_BE_ESCAPED.contains(c)) {
            out.push(c);
        }
    }
    out
}
