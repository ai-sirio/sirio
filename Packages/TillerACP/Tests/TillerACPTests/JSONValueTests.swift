import Testing
import Foundation
@testable import TillerACP

@Suite struct JSONValueTests {
    @Test func decodesNestedObject() throws {
        let data = Data(#"{"a":1,"b":"x","c":[true,null],"d":{"e":2.5}}"#.utf8)
        let value = try JSONDecoder().decode(JSONValue.self, from: data)
        #expect(value["a"]?.intValue == 1)
        #expect(value["b"]?.stringValue == "x")
        #expect(value["c"]?.arrayValue?.first?.boolValue == true)
        #expect(value["c"]?.arrayValue?.last == .null)
        #expect(value["d"]?["e"] == .number(2.5))
        #expect(value["missing"] == nil)
    }

    @Test func roundTripsThroughCodable() throws {
        let original: JSONValue = .object([
            "list": .array([.string("a"), .number(1), .bool(false)]),
            "nested": .object(["k": .null])
        ])
        let encoded = try JSONEncoder().encode(original)
        let decoded = try JSONDecoder().decode(JSONValue.self, from: encoded)
        #expect(decoded == original)
    }

    struct Sample: Codable, Equatable { var name: String; var count: Int }

    @Test func bridgesTypedValues() throws {
        let value = try JSONValue.encoding(Sample(name: "t", count: 3))
        #expect(value["name"]?.stringValue == "t")
        let back = try value.decoded(Sample.self)
        #expect(back == Sample(name: "t", count: 3))
    }
}
