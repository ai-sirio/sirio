// Tiller/App/RunningDots.swift
import SwiftUI

/// Three dots that sweep in sequence — each fades .28 → 1 and lifts slightly,
/// staggered so the highlight travels left→right and loops forever. Marks a
/// running agent; tinted by the agent's color. Honors Reduce Motion.
struct RunningDots: View {
    var color: Color
    var dotSize: CGFloat = 4

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var animating = false

    var body: some View {
        HStack(spacing: 3) {
            ForEach(0..<3, id: \.self) { index in
                Circle()
                    .fill(color)
                    .frame(width: dotSize, height: dotSize)
                    .opacity(reduceMotion ? 0.8 : (animating ? 1 : 0.28))
                    .scaleEffect(reduceMotion ? 1 : (animating ? 1.18 : 1))
                    .animation(
                        reduceMotion
                            ? nil
                            : .easeInOut(duration: 0.525)
                                .repeatForever(autoreverses: true)
                                .delay(Double(index) * 0.14),
                        value: animating
                    )
            }
        }
        .onAppear { animating = true }
    }
}
