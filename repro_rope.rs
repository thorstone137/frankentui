use unicode_segmentation::UnicodeSegmentation;

fn grapheme_to_char_idx_original(text: &str, grapheme_idx: usize) -> usize {
    let mut char_idx = 0usize;
    let mut g_idx = 0usize;
    for grapheme in text.graphemes(true) {
        if g_idx == grapheme_idx {
            return char_idx;
        }
        char_idx += grapheme.chars().count();
        g_idx += 1;
    }
    char_idx
}

fn grapheme_to_char_idx_lines(text: &str, grapheme_idx: usize) -> usize {
    let mut g_count = 0;
    let mut char_count = 0;
    
    // Simulate rope.lines()
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    
    for line in lines {
        let line_g_count = line.graphemes(true).count();
        if g_count + line_g_count > grapheme_idx {
            // Target is in this line
            let offset_in_line = grapheme_idx - g_count;
            let mut current_g = 0;
            for g in line.graphemes(true) {
                if current_g == offset_in_line {
                    return char_count;
                }
                char_count += g.chars().count();
                current_g += 1;
            }
        }
        g_count += line_g_count;
        char_count += line.chars().count();
    }
    char_count
}

fn main() {
    let text = "a\r\nb\nc";
    // Graphemes: "a", "\r\n", "b", "\n", "c" -> 0, 1, 2, 3, 4
    
    for i in 0..6 {
        let orig = grapheme_to_char_idx_original(text, i);
        let new = grapheme_to_char_idx_lines(text, i);
        println!("Index {}: Orig={}, New={}", i, orig, new);
        assert_eq!(orig, new);
    }
    
    let text2 = "hello";
    for i in 0..6 {
        let orig = grapheme_to_char_idx_original(text2, i);
        let new = grapheme_to_char_idx_lines(text2, i);
        assert_eq!(orig, new);
    }
}
