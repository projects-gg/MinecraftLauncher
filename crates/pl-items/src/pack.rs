//! Packing icon images into one atlas: shelves of equal-height rows, at least one transparent pixel
//! between icons so scaled sampling never bleeds into a neighbour, every icon on even pixels.

use crate::image::Image;

const PAD: u32 = 1;

/// Up to the next even number. An icon drawn at half its size (a 32 px drawing shown 16 px wide)
/// then keeps the same pixels wherever it lies in whichever atlas: an odd place shifts the
/// browser's nearest-neighbour pick by one pixel.
fn even(v: u32) -> u32 {
    v + (v & 1)
}

/// Packs images into an atlas `width` wide; returns the atlas and each image's `[x, y, w, h]`.
pub fn pack(images: &[&Image], width: u32) -> (Image, Vec<[u32; 4]>) {
    let mut order: Vec<usize> = (0..images.len()).collect();
    // Tall first, then in the given order (callers pass a stable order).
    order.sort_by(|&a, &b| images[b].height.cmp(&images[a].height).then(a.cmp(&b)));
    let mut rects = vec![[0u32; 4]; images.len()];
    let start = even(PAD);
    let (mut x, mut y, mut row) = (start, start, 0u32);
    for &i in &order {
        let img = images[i];
        if x + img.width + PAD > width {
            x = start;
            y = even(y + row + PAD);
            row = 0;
        }
        rects[i] = [x, y, img.width, img.height];
        x = even(x + img.width + PAD);
        row = row.max(img.height);
    }
    let height = y + row + PAD;
    let mut atlas = Image::new(width, height.max(1));
    for (i, img) in images.iter().enumerate() {
        atlas.blit(img, rects[i][0], rects[i][1]);
    }
    (atlas, rects)
}
