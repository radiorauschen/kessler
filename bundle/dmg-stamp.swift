import CoreGraphics
import CoreText
import Foundation
import ImageIO

let args = CommandLine.arguments
guard args.count == 7, let scale = Double(args[3]) else {
    FileHandle.standardError.write("usage: dmg-stamp.swift in.png out.png scale font version commit\n".data(using: .utf8)!)
    exit(2)
}
let (inPath, outPath, fontPath, version, commit) = (args[1], args[2], args[4], args[5], args[6])
let s = CGFloat(scale)

guard let source = CGImageSourceCreateWithURL(URL(fileURLWithPath: inPath) as CFURL, nil),
      let picture = CGImageSourceCreateImageAtIndex(source, 0, nil) else { exit(1) }
let width = picture.width, height = picture.height
guard let space = CGColorSpace(name: CGColorSpace.sRGB),
      let context = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
                              space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { exit(1) }
context.draw(picture, in: CGRect(x: 0, y: 0, width: width, height: height))

var font = CTFontCreateWithName("Helvetica" as CFString, 11 * s, nil)
if let descriptors = CTFontManagerCreateFontDescriptorsFromURL(URL(fileURLWithPath: fontPath) as CFURL) as? [CTFontDescriptor],
   let first = descriptors.first {
    font = CTFontCreateWithFontDescriptor(first, 11 * s, nil)
}

func colour(_ r: CGFloat, _ g: CGFloat, _ b: CGFloat) -> CGColor {
    CGColor(srgbRed: r / 255, green: g / 255, blue: b / 255, alpha: 1)
}
let parts: [(String, CGColor)] = [
    ("kessler", colour(0x2B, 0x2A, 0x27)),
    (version, colour(0x2E, 0x7D, 0x5A)),
    (commit, colour(0xC2, 0x5E, 0x3A)),
]

var x = 17 * s
let baseline = CGFloat(height) - 24 * s
for (words, tone) in parts where !words.isEmpty {
    let attributes: [NSAttributedString.Key: Any] = [
        NSAttributedString.Key(kCTFontAttributeName as String): font,
        NSAttributedString.Key(kCTForegroundColorAttributeName as String): tone,
        NSAttributedString.Key(kCTKernAttributeName as String): 0.4 * s,
    ]
    let line = CTLineCreateWithAttributedString(NSAttributedString(string: words, attributes: attributes))
    context.textPosition = CGPoint(x: x, y: baseline)
    CTLineDraw(line, context)
    x += CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil)) + 7 * s
}

guard let stamped = context.makeImage(),
      let destination = CGImageDestinationCreateWithURL(URL(fileURLWithPath: outPath) as CFURL, "public.png" as CFString, 1, nil)
else { exit(1) }
CGImageDestinationAddImage(destination, stamped, nil)
exit(CGImageDestinationFinalize(destination) ? 0 : 1)
