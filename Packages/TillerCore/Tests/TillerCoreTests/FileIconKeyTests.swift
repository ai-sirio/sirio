import Testing
import TillerCore

struct FileIconKeyTests {
    @Test func knownExtensionsMap() {
        #expect(FileIconKey.key(forFileName: "main.swift") == .swift)
        #expect(FileIconKey.key(forFileName: "app.tsx") == .react)
        #expect(FileIconKey.key(forFileName: "styles.scss") == .sass)
        #expect(FileIconKey.key(forFileName: "notes.md") == .markdown)
    }

    @Test func extensionMatchIsCaseInsensitive() {
        #expect(FileIconKey.key(forFileName: "photo.JPEG") == .image)
        #expect(FileIconKey.key(forFileName: "Main.SWIFT") == .swift)
    }

    @Test func exactFileNamesBeatExtensions() {
        #expect(FileIconKey.key(forFileName: "Dockerfile") == .docker)
        #expect(FileIconKey.key(forFileName: "Makefile") == .makefile)
        #expect(FileIconKey.key(forFileName: ".gitignore") == .git)
        #expect(FileIconKey.key(forFileName: ".env") == .env)
    }

    @Test func unknownFileFallsBackToGenericFile() {
        #expect(FileIconKey.key(forFileName: "data.xyzabc") == .file)
        #expect(FileIconKey.key(forFileName: "noextension") == .file)
    }

    @Test func specialDirectoriesMap() {
        #expect(FileIconKey.key(forDirectoryName: "src") == .folderSrc)
        #expect(FileIconKey.key(forDirectoryName: "Tests") == .folderTests)
        #expect(FileIconKey.key(forDirectoryName: ".github") == .folderGithub)
        #expect(FileIconKey.key(forDirectoryName: "node_modules") == .folderNodeModules)
    }

    @Test func unknownDirectoryFallsBackToGenericFolder() {
        #expect(FileIconKey.key(forDirectoryName: "Whatever") == .folder)
    }
}
