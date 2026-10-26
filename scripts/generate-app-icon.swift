#!/usr/bin/env swift
//
// generate-app-icon.swift — render the Micro OS macOS App icon.
//
// The icon is a "handheld device launcher": a macOS squircle holding the
// 800x480 device bezel, its paper screen, the OS app bar and the 2x2 launcher
// card grid from apps/shell. The small sizes (16/32) drop the device bezel and
// keep only the squircle + four cards, which stays legible in menus.
//
// Output (committed):
//   assets/app-icon/AppIcon.icns
//   assets/app-icon/icon-1024.png   source preview
//   assets/app-icon/icon-512.png    embedded at runtime for the Dock icon
//   assets/app-icon/icon-128.png    README / docs preview
//   assets/app-icon/icon-32.png     32 px preview (compact composition)
//   assets/app-icon/icon-16.png     16 px preview (compact composition)
//
// Run:  swift scripts/generate-app-icon.swift
//
import AppKit
import CoreGraphics
import Foundation

// MARK: - Palette (matches apps/shell + products/micro-web-player)

func srgb(_ hex: UInt32, _ alpha: CGFloat = 1) -> CGColor {
    let r = CGFloat((hex >> 16) & 0xFF) / 255
    let g = CGFloat((hex >> 8) & 0xFF) / 255
    let b = CGFloat(hex & 0xFF) / 255
    return CGColor(colorSpace: CGColorSpace(name: CGColorSpace.sRGB)!, components: [r, g, b, alpha])!
}

let ink = srgb(0x101820)
let paper = srgb(0xFFFDF7)
let canvas = srgb(0xF4EFE4)
let sage = srgb(0xA7CF84)
let evergreen = srgb(0x1E7256)
let muted = srgb(0x84937D)
let terracotta = srgb(0xC0683B)
let deep = srgb(0x0C1713)

// MARK: - Geometry helpers

/// Superellipse (n = 5) approximating the macOS continuous-corner squircle.
func squircle(in rect: CGRect, exponent n: CGFloat = 5) -> CGPath {
    let path = CGMutablePath()
    let cx = rect.midX, cy = rect.midY
    let a = rect.width / 2, b = rect.height / 2
    let steps = 720
    for step in 0...steps {
        let t = CGFloat(step) / CGFloat(steps) * 2 * .pi
        let ct = cos(t), st = sin(t)
        let x = cx + a * (ct < 0 ? -1 : 1) * pow(abs(ct), 2 / n)
        let y = cy + b * (st < 0 ? -1 : 1) * pow(abs(st), 2 / n)
        if step == 0 { path.move(to: CGPoint(x: x, y: y)) }
        else { path.addLine(to: CGPoint(x: x, y: y)) }
    }
    path.closeSubpath()
    return path
}

func rounded(_ rect: CGRect, _ radius: CGFloat) -> CGPath {
    CGPath(roundedRect: rect, cornerWidth: radius, cornerHeight: radius, transform: nil)
}

func fill(_ ctx: CGContext, _ path: CGPath, _ color: CGColor) {
    ctx.saveGState()
    ctx.addPath(path)
    ctx.setFillColor(color)
    ctx.fillPath()
    ctx.restoreGState()
}

/// Keep a stroke visible even when the 1024-unit design is rasterised small.
func hairline(_ designWidth: CGFloat, _ scale: CGFloat) -> CGFloat {
    max(designWidth, 0.85 / scale)
}

func fill(_ ctx: CGContext, _ rect: CGRect, _ color: CGColor) {
    fill(ctx, CGPath(rect: rect, transform: nil), color)
}

func stroke(_ ctx: CGContext, _ path: CGPath, _ color: CGColor, width: CGFloat) {
    ctx.saveGState()
    ctx.addPath(path)
    ctx.setStrokeColor(color)
    ctx.setLineWidth(width)
    ctx.setLineJoin(.round)
    ctx.strokePath()
    ctx.restoreGState()
}

func verticalGradient(_ ctx: CGContext, in rect: CGRect, from: CGColor, to: CGColor) {
    let space = CGColorSpace(name: CGColorSpace.sRGB)!
    guard let gradient = CGGradient(colorsSpace: space, colors: [from, to] as CFArray, locations: [0, 1]) else { return }
    ctx.saveGState()
    ctx.clip(to: rect)
    ctx.drawLinearGradient(
        gradient,
        start: CGPoint(x: rect.midX, y: rect.maxY),
        end: CGPoint(x: rect.midX, y: rect.minY),
        options: []
    )
    ctx.restoreGState()
}

func radialGlow(_ ctx: CGContext, center: CGPoint, radius: CGFloat, color: CGColor) {
    let space = CGColorSpace(name: CGColorSpace.sRGB)!
    guard let gradient = CGGradient(
        colorsSpace: space,
        colors: [color, color.copy(alpha: 0)!] as CFArray,
        locations: [0, 1]
    ) else { return }
    ctx.saveGState()
    ctx.drawRadialGradient(
        gradient,
        startCenter: center,
        startRadius: 0,
        endCenter: center,
        endRadius: radius,
        options: []
    )
    ctx.restoreGState()
}

/// Draw a single line of text centred on `center`, using the current CTM.
func drawText(
    _ ctx: CGContext,
    _ text: String,
    center: CGPoint,
    size: CGFloat,
    weight: NSFont.Weight,
    color: CGColor,
    tracking: CGFloat = 0
) {
    let font = NSFont.systemFont(ofSize: size, weight: weight)
    var attributes: [NSAttributedString.Key: Any] = [
        .font: font,
        .foregroundColor: NSColor(cgColor: color)!,
    ]
    if tracking != 0 { attributes[.kern] = tracking }
    let attributed = NSAttributedString(string: text, attributes: attributes)
    let line = CTLineCreateWithAttributedString(attributed)
    let bounds = CTLineGetBoundsWithOptions(line, .useOpticalBounds)
    ctx.saveGState()
    ctx.textMatrix = .identity
    ctx.textPosition = CGPoint(
        x: center.x - bounds.width / 2 - bounds.minX,
        y: center.y - bounds.height / 2 - bounds.minY
    )
    CTLineDraw(line, ctx)
    ctx.restoreGState()
}

/// WiFi fan: a dot with three upper arcs, centred on `center`.
func drawWifi(_ ctx: CGContext, center: CGPoint, color: CGColor, scale: CGFloat) {
    ctx.saveGState()
    ctx.setStrokeColor(color)
    ctx.setFillColor(color)
    ctx.setLineCap(.round)
    let dotRadius = 2.2 * scale
    ctx.fillEllipse(in: CGRect(
        x: center.x - dotRadius, y: center.y - 8 * scale - dotRadius,
        width: dotRadius * 2, height: dotRadius * 2
    ))
    for radius in [7.0, 12.0, 17.0] {
        ctx.setLineWidth(2.6 * scale)
        ctx.addArc(
            center: CGPoint(x: center.x, y: center.y - 8 * scale),
            radius: radius * scale,
            startAngle: .pi * 0.25,
            endAngle: .pi * 0.75,
            clockwise: false
        )
        ctx.strokePath()
    }
    ctx.restoreGState()
}

// MARK: - Composition

struct Card {
    let rect: CGRect
    let accent: Bool
    let glyph: String
}

func drawLauncherCard(
    _ ctx: CGContext,
    card: Card,
    detail: Bool,
    showGlyph: Bool,
    scale: CGFloat
) {
    let rect = card.rect
    let radius = min(rect.width, rect.height) * 0.13
    let path = rounded(rect, radius)

    if card.accent {
        fill(ctx, path, terracotta)
        stroke(ctx, path, srgb(0xE0896A, 0.9), width: hairline(2, scale))
        // Soft top highlight so the tile reads as a raised surface.
        ctx.saveGState()
        ctx.addPath(path)
        ctx.clip()
        verticalGradient(ctx, in: rect, from: srgb(0xCF7C4F), to: terracotta)
        ctx.restoreGState()
    } else {
        fill(ctx, path, srgb(0xF2F2E7))
        stroke(ctx, path, srgb(0x648472), width: hairline(2, scale))
        // Paper top light.
        ctx.saveGState()
        ctx.addPath(path)
        ctx.clip()
        verticalGradient(ctx, in: rect, from: srgb(0xFFFFFF, 0.55), to: srgb(0xFFFFFF, 0))
        ctx.restoreGState()
    }

    guard detail else {
        // 16/32 px: one chunky bar per card is all that survives.
        let bar = CGRect(
            x: rect.minX + rect.width * 0.18,
            y: rect.midY - rect.height * 0.07,
            width: rect.width * 0.44,
            height: max(2, rect.height * 0.14)
        )
        fill(ctx, rounded(bar, bar.height / 2), card.accent ? paper : ink)
        return
    }

    let midY = rect.midY
    let badgeDiameter = rect.height * 0.42
    let badgeRect = CGRect(
        x: rect.minX + rect.width * 0.07,
        y: midY - badgeDiameter / 2,
        width: badgeDiameter,
        height: badgeDiameter
    )
    let badgeColor = card.accent ? paper : srgb(0xE7F0E5)
    fill(ctx, CGPath(ellipseIn: badgeRect, transform: nil), badgeColor)
    stroke(ctx, CGPath(ellipseIn: badgeRect, transform: nil), card.accent ? srgb(0xE0896A) : sage, width: hairline(1.6, scale))
    if !card.accent {
        // Faint sage field inside the badge, matching .micro-icon[data-variant=badge].
        fill(ctx, CGPath(ellipseIn: badgeRect.insetBy(dx: 3, dy: 3), transform: nil), srgb(0xE7F0E5))
    }
    if showGlyph {
        drawText(
            ctx,
            card.glyph,
            center: CGPoint(x: badgeRect.midX, y: badgeRect.midY + badgeDiameter * 0.04),
            size: badgeDiameter * 0.62,
            weight: .semibold,
            color: card.accent ? terracotta : evergreen
        )
    }

    let textX = badgeRect.maxX + rect.width * 0.07
    let titleHeight = rect.height * 0.11
    let title = CGRect(
        x: textX, y: midY + rect.height * 0.035,
        width: rect.width * 0.40, height: titleHeight
    )
    let caption = CGRect(
        x: textX, y: midY - rect.height * 0.13,
        width: rect.width * 0.27, height: rect.height * 0.075
    )
    fill(ctx, rounded(title, titleHeight / 2), card.accent ? srgb(0xFFFDF7, 0.95) : srgb(0x173128))
    fill(ctx, rounded(caption, caption.height / 2), card.accent ? srgb(0xFFFDF7, 0.62) : muted)
}

func drawIcon(_ ctx: CGContext, pixelSize: Int) {
    let scale = CGFloat(pixelSize) / 1024.0
    let detail = pixelSize >= 64
    let text = pixelSize >= 256
    let body = CGRect(x: 100, y: 100, width: 824, height: 824)
    let bodyPath = squircle(in: body)

    // Drop shadow under the squircle.
    ctx.saveGState()
    ctx.setShadow(
        offset: CGSize(width: 0, height: -18),
        blur: 42,
        color: srgb(0x000000, 0.38)
    )
    fill(ctx, bodyPath, deep)
    ctx.restoreGState()

    // Squircle body: deep evergreen with a sage glow from the top left.
    ctx.saveGState()
    ctx.addPath(bodyPath)
    ctx.clip()
    verticalGradient(ctx, in: body, from: srgb(0x2A5A49), to: srgb(0x0B1512))
    radialGlow(ctx, center: CGPoint(x: 340, y: 780), radius: 520, color: srgb(0xA7CF84, 0.20))
    radialGlow(ctx, center: CGPoint(x: 800, y: 250), radius: 380, color: srgb(0xC0683B, 0.10))
    ctx.restoreGState()

    if detail {
        drawDevice(ctx, scale: scale, detail: detail, text: text)
    } else {
        drawCompactGrid(ctx, body: body, scale: scale)
    }

    // Inner rim: a hairline of light along the squircle edge.
    stroke(
        ctx,
        squircle(in: body.insetBy(dx: 7, dy: 7)),
        srgb(0xFFFFFF, 0.13),
        width: hairline(2, scale)
    )
}

/// Full composition: bezel, paper screen, OS app bar and the 2x2 launcher grid.
func drawDevice(_ ctx: CGContext, scale: CGFloat, detail: Bool, text: Bool) {
    let bezel = CGRect(x: 162, y: 300, width: 700, height: 424)
    let bezelPath = rounded(bezel, 48)

    ctx.saveGState()
    ctx.setShadow(offset: CGSize(width: 0, height: -14), blur: 30, color: srgb(0x000000, 0.42))
    fill(ctx, bezelPath, srgb(0x0A1411))
    ctx.restoreGState()

    ctx.saveGState()
    ctx.addPath(bezelPath)
    ctx.clip()
    verticalGradient(ctx, in: bezel, from: srgb(0x1B2C25), to: srgb(0x070E0B))
    ctx.restoreGState()
    stroke(ctx, bezelPath, srgb(0x4A5F54, 0.75), width: hairline(2, scale))

    // Camera + power LED live in the bezel, not the screen.
    fill(ctx, CGPath(ellipseIn: CGRect(x: 505, y: 705, width: 14, height: 14), transform: nil), srgb(0x20332B))
    fill(ctx, CGPath(ellipseIn: CGRect(x: 508.5, y: 708.5, width: 4, height: 4), transform: nil), srgb(0x6E8C7C, 0.8))
    ctx.saveGState()
    ctx.setShadow(offset: .zero, blur: 9, color: srgb(0xC0683B, 0.95))
    fill(ctx, CGPath(ellipseIn: CGRect(x: 846, y: 508, width: 12, height: 12), transform: nil), terracotta)
    ctx.restoreGState()

    // Screen.
    let screen = CGRect(x: 182, y: 320, width: 660, height: 384)
    let screenPath = rounded(screen, 30)
    ctx.saveGState()
    ctx.addPath(screenPath)
    ctx.clip()
    verticalGradient(ctx, in: screen, from: srgb(0xEDF3E7), to: canvas)
    ctx.restoreGState()

    // App bar (40 px of the 480 px device height). Clip to the screen so its
    // top corners follow the rounded bezel instead of squaring it off.
    let bar = CGRect(x: 182, y: 664, width: 660, height: 40)
    ctx.saveGState()
    ctx.addPath(screenPath)
    ctx.clip()
    fill(ctx, bar, srgb(0xE7EAE4))
    fill(
        ctx,
        CGRect(x: bar.minX, y: bar.minY, width: bar.width, height: max(1, 1.6 * scale)),
        srgb(0xD5D9CE)
    )
    ctx.restoreGState()
    if text {
        drawWifi(ctx, center: CGPoint(x: 222, y: 686), color: evergreen, scale: 0.95)
        drawText(ctx, "Home", center: CGPoint(x: 512, y: 684), size: 21, weight: .semibold, color: ink)
        drawText(ctx, "9:41", center: CGPoint(x: 790, y: 684), size: 18, weight: .medium, color: ink, tracking: 0.2)
    } else {
        // Below 256 px the title/clock turn to mush: keep a quiet title bar.
        let titleBar = CGRect(x: 452, y: 679, width: 120, height: 10)
        fill(ctx, rounded(titleBar, 5), srgb(0x101820, 0.22))
    }

    // Four launcher cards: accent first, then Counter-free glyphs that read as
    // "apps", "settings" and "more".
    let cards: [Card] = [
        Card(rect: CGRect(x: 208, y: 501, width: 295, height: 137), accent: true, glyph: "+"),
        Card(rect: CGRect(x: 521, y: 501, width: 295, height: 137), accent: false, glyph: "⚙︎"),
        Card(rect: CGRect(x: 208, y: 346, width: 295, height: 137), accent: false, glyph: "墨"),
        Card(rect: CGRect(x: 521, y: 346, width: 295, height: 137), accent: false, glyph: "···"),
    ]
    for card in cards {
        drawLauncherCard(ctx, card: card, detail: detail, showGlyph: text, scale: scale)
    }

    // Glass glare across the top-left of the screen.
    ctx.saveGState()
    ctx.addPath(screenPath)
    ctx.clip()
    let glare = CGGradient(
        colorsSpace: CGColorSpace(name: CGColorSpace.sRGB)!,
        colors: [srgb(0xFFFFFF, 0.16), srgb(0xFFFFFF, 0)] as CFArray,
        locations: [0, 1]
    )!
    ctx.drawLinearGradient(
        glare,
        start: CGPoint(x: 182, y: 704),
        end: CGPoint(x: 560, y: 430),
        options: []
    )
    ctx.restoreGState()

    stroke(ctx, screenPath, srgb(0x000000, 0.35), width: hairline(3, scale))
}

/// 16/32 px: squircle + four large cards, no bezel or text.
func drawCompactGrid(_ ctx: CGContext, body: CGRect, scale: CGFloat) {
    let inset = body.insetBy(dx: 132, dy: 132)
    let gap = inset.width * 0.055
    let cardWidth = (inset.width - gap) / 2
    let cardHeight = (inset.height - gap) / 2
    let accentIndex = 0
    let glyphs = ["", "", "", ""]
    for row in 0..<2 {
        for column in 0..<2 {
            let index = row * 2 + column
            let rect = CGRect(
                x: inset.minX + CGFloat(column) * (cardWidth + gap),
                y: inset.maxY - cardHeight - CGFloat(row) * (cardHeight + gap),
                width: cardWidth,
                height: cardHeight
            )
            drawLauncherCard(
                ctx,
                card: Card(rect: rect, accent: index == accentIndex, glyph: glyphs[index]),
                detail: false,
                showGlyph: false,
                scale: scale
            )
        }
    }
}

// MARK: - Rendering

func renderPNG(pixelSize: Int) -> Data {
    let colorSpace = CGColorSpace(name: CGColorSpace.sRGB)!
    guard let ctx = CGContext(
        data: nil,
        width: pixelSize,
        height: pixelSize,
        bitsPerComponent: 8,
        bytesPerRow: 0,
        space: colorSpace,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    ) else {
        fatalError("cannot create \(pixelSize)px context")
    }
    ctx.setAllowsAntialiasing(true)
    ctx.setShouldAntialias(true)
    ctx.interpolationQuality = .high
    ctx.scaleBy(x: CGFloat(pixelSize) / 1024, y: CGFloat(pixelSize) / 1024)
    drawIcon(ctx, pixelSize: pixelSize)

    guard let image = ctx.makeImage() else { fatalError("cannot snapshot \(pixelSize)px icon") }
    let rep = NSBitmapImageRep(cgImage: image)
    rep.size = NSSize(width: pixelSize, height: pixelSize)
    guard let data = rep.representation(using: .png, properties: [:]) else {
        fatalError("cannot encode \(pixelSize)px PNG")
    }
    return data
}

// MARK: - Main

let root = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()
    .deletingLastPathComponent()
let output = root.appendingPathComponent("assets/app-icon")
try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)

func write(_ data: Data, to name: String) throws {
    try data.write(to: output.appendingPathComponent(name))
    print("  \(name)")
}

print("Micro OS app icon → \(output.path)")
for (name, size) in [
    ("icon-1024.png", 1024),
    ("icon-512.png", 512),
    ("icon-128.png", 128),
    ("icon-32.png", 32),
    ("icon-16.png", 16),
] {
    try write(renderPNG(pixelSize: size), to: name)
}

// .icns container: PNG-backed elements only, including the small 16/32 slots
// (icp4/icp5) so their transparency and the compact artwork survive unchanged.
// macOS reads these directly; iconutil is not needed (and re-flattens the
// small sizes onto white when it exports them).
func writePNGContainerICNS(to url: URL) throws {
    func appendUInt32BE(_ value: UInt32, to data: inout Data) {
        data.append(UInt8((value >> 24) & 0xFF))
        data.append(UInt8((value >> 16) & 0xFF))
        data.append(UInt8((value >> 8) & 0xFF))
        data.append(UInt8(value & 0xFF))
    }
    let elements: [(String, Int)] = [
        ("icp4", 16), ("ic11", 32), ("icp5", 32), ("ic12", 64),
        ("ic07", 128), ("ic13", 256), ("ic08", 256), ("ic14", 512),
        ("ic09", 512), ("ic10", 1024),
    ]
    var icns = Data("icns".utf8)
    appendUInt32BE(0, to: &icns)
    for (type, size) in elements {
        let png = renderPNG(pixelSize: size)
        icns.append(contentsOf: type.utf8)
        appendUInt32BE(UInt32(png.count + 8), to: &icns)
        icns.append(png)
    }
    icns.replaceSubrange(4..<8, with: withUnsafeBytes(of: UInt32(icns.count).bigEndian) { Data($0) })
    try icns.write(to: url)
}

let icns = output.appendingPathComponent("AppIcon.icns")
try writePNGContainerICNS(to: icns)
print("  AppIcon.icns")
