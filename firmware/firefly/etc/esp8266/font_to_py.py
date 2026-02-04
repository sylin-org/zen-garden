#!/usr/bin/env python3
"""
Convert a TTF/OTF font to a MicroPython module for use with SSD1306 framebuffer displays.

Outputs a .py file containing glyph bitmaps and width data, compatible with
Peter Hinch's Writer class pattern.

Usage:
    python font_to_py.py <font_file> <height_px> <output.py> [--chars CHARS]

Example:
    python font_to_py.py minecraft.ttf 8 minecraft_8.py
    python font_to_py.py minecraft.ttf 16 minecraft_16.py
"""

import sys
import argparse
import freetype

# Default character set: printable ASCII
DEFAULT_CHARS = ''.join(chr(c) for c in range(32, 127))


def render_glyph(face, char, height):
    """Render a single character and return (bitmap_bytes, width, height)."""
    face.set_pixel_sizes(0, height)
    face.load_char(char, freetype.FT_LOAD_RENDER | freetype.FT_LOAD_TARGET_MONO)
    
    glyph = face.glyph
    bitmap = glyph.bitmap
    
    # Advance width in pixels (26.6 fixed point)
    advance = glyph.advance.x >> 6
    
    # Bitmap dimensions
    bmp_width = bitmap.width
    bmp_rows = bitmap.rows
    bmp_left = glyph.bitmap_left
    bmp_top = glyph.bitmap_top
    
    # Create a canvas of (advance x height) pixels
    canvas_width = max(advance, bmp_left + bmp_width, 1)
    canvas_height = height
    canvas = bytearray(canvas_width * canvas_height)
    
    # Calculate baseline position (ascender from top)
    face.set_pixel_sizes(0, height)
    ascender = face.size.ascender >> 6
    
    # Copy bitmap onto canvas
    for row in range(bmp_rows):
        y = ascender - bmp_top + row
        if y < 0 or y >= canvas_height:
            continue
        for col in range(bmp_width):
            x = bmp_left + col
            if x < 0 or x >= canvas_width:
                continue
            # Mono bitmap: 1 bit per pixel, packed into bytes
            byte_index = row * bitmap.pitch + (col >> 3)
            bit_index = 7 - (col & 7)
            if byte_index < len(bitmap.buffer):
                if bitmap.buffer[byte_index] & (1 << bit_index):
                    canvas[y * canvas_width + x] = 1
    
    return canvas, canvas_width, canvas_height


def pack_horizontal(canvas, width, height):
    """Pack pixel data into bytes, horizontal bit mapping (MSB first).
    Each row is padded to a byte boundary."""
    data = bytearray()
    for y in range(height):
        for x_byte in range((width + 7) // 8):
            byte = 0
            for bit in range(8):
                x = x_byte * 8 + bit
                if x < width and canvas[y * width + x]:
                    byte |= (1 << (7 - bit))
            data.append(byte)
    return data


def convert_font(font_path, height, chars):
    """Convert font to glyph data."""
    face = freetype.Face(font_path)
    
    glyphs = {}
    max_width = 0
    
    for char in chars:
        canvas, width, canvas_height = render_glyph(face, char, height)
        packed = pack_horizontal(canvas, width, canvas_height)
        glyphs[char] = {
            'width': width,
            'data': packed,
        }
        max_width = max(max_width, width)
    
    return glyphs, max_width, height


def generate_module(glyphs, max_width, height, chars):
    """Generate a MicroPython module string."""
    lines = []
    lines.append('# Auto-generated font module')
    lines.append(f'# Height: {height}px, Max width: {max_width}px')
    lines.append(f'# Characters: {len(chars)}')
    lines.append('')
    lines.append('import framebuf')
    lines.append('')
    lines.append(f'HEIGHT = {height}')
    lines.append(f'MAX_WIDTH = {max_width}')
    lines.append('')
    
    # Build the font data as a dictionary of (width, bytes)
    lines.append('# Glyph data: ord -> (width, bytes)')
    lines.append('# Bytes are horizontal bit-packed, MSB first, row-major')
    lines.append('_GLYPHS = {')
    
    for char in chars:
        g = glyphs[char]
        data_hex = ', '.join(f'0x{b:02x}' for b in g['data'])
        if char == '\\':
            char_repr = "ord('\\\\')"
        elif char == "'":
            char_repr = 'ord("\'")' 
        else:
            char_repr = f"ord('{char}')"
        lines.append(f'    {char_repr}: ({g["width"]}, bytes([{data_hex}])),')
    
    lines.append('}')
    lines.append('')
    
    # Writer-compatible API
    lines.append('''
def get_ch(ch):
    """Return (memoryview_of_glyph_buffer, height, width) for a character."""
    entry = _GLYPHS.get(ord(ch))
    if entry is None:
        entry = _GLYPHS.get(ord('?'))
    if entry is None:
        # Fallback: empty glyph
        w = MAX_WIDTH
        return memoryview(bytearray((w + 7) // 8 * HEIGHT)), HEIGHT, w
    width, data = entry
    return memoryview(data), HEIGHT, width


def text_width(s):
    """Calculate pixel width of a string."""
    total = 0
    for ch in s:
        entry = _GLYPHS.get(ord(ch))
        if entry:
            total += entry[0] + 1  # +1 for inter-char spacing
        else:
            total += MAX_WIDTH + 1
    if total > 0:
        total -= 1  # Remove trailing space
    return total
''')
    
    return '\n'.join(lines)


def main():
    parser = argparse.ArgumentParser(description='Convert TTF to MicroPython font module')
    parser.add_argument('font', help='TTF/OTF font file path')
    parser.add_argument('height', type=int, help='Font height in pixels')
    parser.add_argument('output', help='Output .py file path')
    parser.add_argument('--chars', default=DEFAULT_CHARS,
                        help='Characters to include (default: printable ASCII)')
    parser.add_argument('--extra', default='',
                        help='Extra characters to include beyond defaults')
    
    args = parser.parse_args()
    chars = args.chars + args.extra
    
    print(f'Converting {args.font} at {args.height}px...')
    print(f'Characters: {len(chars)}')
    
    glyphs, max_width, height = convert_font(args.font, args.height, chars)
    module = generate_module(glyphs, max_width, height, chars)
    
    with open(args.output, 'w') as f:
        f.write(module)
    
    # Stats
    total_bytes = sum(len(g['data']) for g in glyphs.values())
    print(f'Max glyph width: {max_width}px')
    print(f'Total glyph data: {total_bytes} bytes')
    print(f'Output: {args.output}')
    
    # Show preview of a few characters
    print('\nPreview:')
    for preview_char in ['A', 'g', 'i', 'W', '@']:
        if preview_char in glyphs:
            g = glyphs[preview_char]
            print(f"\n  '{preview_char}' ({g['width']}px wide):")
            bytes_per_row = (g['width'] + 7) // 8
            for y in range(height):
                row = ''
                for x in range(g['width']):
                    byte_idx = y * bytes_per_row + (x >> 3)
                    bit_idx = 7 - (x & 7)
                    if g['data'][byte_idx] & (1 << bit_idx):
                        row += '██'
                    else:
                        row += '  '
                print(f'    {row}')


if __name__ == '__main__':
    main()
