// How this app is signed: Connect shows the quarantine hint only for ad-hoc builds (spec §3.4),
// which macOS may block when an AI app launches the bundled CLI.

import Foundation
import Security

public enum CodeSigning {
    /// Whether the running app is ad-hoc signed (or not signed at all), i.e. not notarized with
    /// a Developer ID. Read once from the code signature; no network, no keychain.
    public static let isAdHocOrUnsigned: Bool = {
        var code: SecCode?
        guard SecCodeCopySelf([], &code) == errSecSuccess, let code else { return true }
        var staticCode: SecStaticCode?
        guard SecCodeCopyStaticCode(code, [], &staticCode) == errSecSuccess, let staticCode else { return true }
        var info: CFDictionary?
        guard SecCodeCopySigningInformation(staticCode, SecCSFlags(rawValue: kSecCSSigningInformation), &info) == errSecSuccess,
              let dictionary = info as? [String: Any]
        else { return true }
        guard let flags = dictionary[kSecCodeInfoFlags as String] as? NSNumber else {
            // No flags: no signature.
            return true
        }
        return SecCodeSignatureFlags(rawValue: flags.uint32Value).contains(.adhoc)
    }()
}
