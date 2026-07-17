import Testing
import TillerCore

struct FileIconThemeTests {
    @Test func sfSymbolsThemeIsAlwaysSystem() {
        for key in FileIconKey.allCases {
            guard case .system(let name) = FileIconTheme.sfSymbols.iconRef(for: key) else {
                Issue.record("expected .system for \(key)")
                return
            }
            #expect(!name.isEmpty)
        }
    }

    @Test func materialThemeUsesAssetsForMappedKeys() {
        #expect(FileIconTheme.material.iconRef(for: .swift) == .asset("mat-swift"))
        #expect(FileIconTheme.material.iconRef(for: .folderSrc) == .asset("mat-folder-src"))
        #expect(FileIconTheme.material.iconRef(for: .folder) == .asset("mat-folder-base"))
        #expect(FileIconTheme.material.iconRef(for: .file) == .asset("mat-document"))
    }

    @Test func materialThemeFallsBackToSystemForUnmappedKeys() {
        // symlink deliberately has no material asset (spec: link stays SF in both themes)
        #expect(FileIconTheme.material.iconRef(for: .symlink) == .system("link"))
    }

    @Test func everyKeyResolvesToNonEmptyRefInBothThemes() {
        for theme in FileIconTheme.allCases {
            for key in FileIconKey.allCases {
                switch theme.iconRef(for: key) {
                case .system(let name): #expect(!name.isEmpty)
                case .asset(let name): #expect(name.hasPrefix("mat-"))
                }
            }
        }
    }
}
