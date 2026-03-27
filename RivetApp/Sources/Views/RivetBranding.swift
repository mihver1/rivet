import SwiftUI

enum RivetBrandPalette {
    static let graphite = Color(red: 11.0 / 255.0, green: 19.0 / 255.0, blue: 34.0 / 255.0)
    static let navy = Color(red: 19.0 / 255.0, green: 33.0 / 255.0, blue: 56.0 / 255.0)
    static let slate = Color(red: 30.0 / 255.0, green: 49.0 / 255.0, blue: 72.0 / 255.0)
    static let copper = Color(red: 1.0, green: 158.0 / 255.0, blue: 89.0 / 255.0)
    static let copperLight = Color(red: 1.0, green: 208.0 / 255.0, blue: 138.0 / 255.0)
    static let steel = Color(red: 247.0 / 255.0, green: 250.0 / 255.0, blue: 253.0 / 255.0)
    static let steelDark = Color(red: 167.0 / 255.0, green: 179.0 / 255.0, blue: 197.0 / 255.0)
    static let signal = Color(red: 61.0 / 255.0, green: 226.0 / 255.0, blue: 208.0 / 255.0)
}

struct RivetBadge: View {
    var size: CGFloat = 72

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: size * 0.25, style: .continuous)
                .fill(
                    LinearGradient(
                        colors: [RivetBrandPalette.graphite, RivetBrandPalette.navy, RivetBrandPalette.slate],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    )
                )

            RoundedRectangle(cornerRadius: size * 0.22, style: .continuous)
                .fill(
                    LinearGradient(
                        colors: [
                            RivetBrandPalette.navy.opacity(0.96),
                            RivetBrandPalette.graphite.opacity(0.98),
                        ],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    )
                )
                .padding(size * 0.035)

            RivetGlowShape()
                .fill(
                    LinearGradient(
                        colors: [
                            RivetBrandPalette.signal.opacity(0.34),
                            RivetBrandPalette.signal.opacity(0.0),
                        ],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    )
                )
                .padding(size * 0.07)

            RivetSignalArc()
                .stroke(RivetBrandPalette.signal.opacity(0.7), style: StrokeStyle(lineWidth: size * 0.02, lineCap: .round))
                .padding(size * 0.18)

            Text("R")
                .font(.system(size: size * 0.66, weight: .black, design: .rounded))
                .foregroundStyle(
                    LinearGradient(
                        colors: [RivetBrandPalette.copperLight, RivetBrandPalette.copper],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    )
                )
                .offset(x: -size * 0.02, y: size * 0.03)
                .shadow(color: RivetBrandPalette.graphite.opacity(0.2), radius: size * 0.03, y: size * 0.015)

            Circle()
                .fill(
                    LinearGradient(
                        colors: [RivetBrandPalette.steel, RivetBrandPalette.steelDark],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    )
                )
                .frame(width: size * 0.19, height: size * 0.19)
                .overlay(
                    Circle()
                        .fill(RivetBrandPalette.graphite.opacity(0.4))
                        .frame(width: size * 0.07, height: size * 0.07)
                )
                .offset(x: -size * 0.01, y: -size * 0.03)

            VStack(spacing: size * 0.08) {
                ForEach(0..<3, id: \.self) { _ in
                    Circle()
                        .fill(RivetBrandPalette.graphite.opacity(0.22))
                        .frame(width: size * 0.045, height: size * 0.045)
                }
            }
            .offset(x: -size * 0.28, y: -size * 0.03)

            RoundedRectangle(cornerRadius: size * 0.22, style: .continuous)
                .strokeBorder(Color.white.opacity(0.08), lineWidth: size * 0.012)
                .padding(size * 0.035)
        }
        .frame(width: size, height: size)
        .shadow(color: RivetBrandPalette.graphite.opacity(0.14), radius: size * 0.08, y: size * 0.035)
        .accessibilityHidden(true)
    }
}

struct RivetBrandLockup: View {
    var badgeSize: CGFloat = 60
    var showsTagline = true

    var body: some View {
        HStack(spacing: badgeSize * 0.22) {
            RivetBadge(size: badgeSize)

            VStack(alignment: .leading, spacing: badgeSize * 0.03) {
                Text("RIVET")
                    .font(.system(size: badgeSize * 0.56, weight: .black, design: .rounded))
                    .tracking(badgeSize * 0.045)
                    .foregroundStyle(RivetBrandPalette.graphite)

                if showsTagline {
                    Text("SECURE SSH CONNECTIONS")
                        .font(.system(size: badgeSize * 0.15, weight: .semibold, design: .rounded))
                        .tracking(badgeSize * 0.04)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .fixedSize(horizontal: false, vertical: true)
    }
}

struct RivetSidebarHeader: View {
    var body: some View {
        HStack(spacing: 12) {
            RivetBadge(size: 44)

            VStack(alignment: .leading, spacing: 2) {
                Text("RIVET")
                    .font(.system(size: 20, weight: .black, design: .rounded))
                    .tracking(1.8)
                    .foregroundStyle(RivetBrandPalette.graphite)

                Text("Secure SSH connections")
                    .font(.system(size: 11, weight: .semibold, design: .rounded))
                    .tracking(1.2)
                    .foregroundStyle(.secondary)
            }

            Spacer()
        }
        .padding(12)
        .background(
            RoundedRectangle(cornerRadius: 16, style: .continuous)
                .fill(.regularMaterial)
        )
    }
}

private struct RivetGlowShape: Shape {
    func path(in rect: CGRect) -> Path {
        var path = Path()
        path.move(to: CGPoint(x: rect.minX + rect.width * 0.06, y: rect.minY + rect.height * 0.18))
        path.addLine(to: CGPoint(x: rect.minX + rect.width * 0.2, y: rect.minY + rect.height * 0.08))
        path.addQuadCurve(
            to: CGPoint(x: rect.minX + rect.width * 0.78, y: rect.minY + rect.height * 0.1),
            control: CGPoint(x: rect.minX + rect.width * 0.42, y: rect.minY - rect.height * 0.03)
        )
        path.addQuadCurve(
            to: CGPoint(x: rect.minX + rect.width * 0.26, y: rect.maxY * 0.74),
            control: CGPoint(x: rect.minX + rect.width * 0.38, y: rect.minY + rect.height * 0.38)
        )
        path.closeSubpath()
        return path
    }
}

private struct RivetSignalArc: Shape {
    func path(in rect: CGRect) -> Path {
        var path = Path()
        path.addArc(
            center: CGPoint(x: rect.midX + rect.width * 0.06, y: rect.midY - rect.height * 0.06),
            radius: rect.width * 0.34,
            startAngle: .degrees(205),
            endAngle: .degrees(338),
            clockwise: false
        )
        return path
    }
}
