// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
#if os(macOS) && FEI_TESTFLIGHT_BETA
import Foundation
import Security

enum TestFlightInstall {
    // Apple re-signs TestFlight downloads. A developer signature or a plist flag
    // cannot satisfy this requirement, even when it uses the same bundle ID.
    static func verified(at url: URL) -> Bool {
        let rule = "anchor apple generic and certificate leaf[subject.CN] = \"TestFlight Beta Distribution\" "
            + "and identifier \"app.feinotes.mac\" and entitlement[\"beta-reports-active\"] exists "
            + "and entitlement[\"com.apple.developer.team-identifier\"] = \"95BC6VVXL8\""
        var requirement: SecRequirement?
        var code: SecStaticCode?
        guard SecRequirementCreateWithString(rule as CFString, [], &requirement) == errSecSuccess,
              SecStaticCodeCreateWithPath(url as CFURL, [], &code) == errSecSuccess,
              let code, let requirement else { return false }
        let flags = SecCSFlags(rawValue: kSecCSStrictValidate | kSecCSCheckAllArchitectures | kSecCSCheckNestedCode)
        guard SecStaticCodeCheckValidity(code, flags, requirement) == errSecSuccess else { return false }
        var info: CFDictionary?
        guard SecCodeCopySigningInformation(code, SecCSFlags(rawValue: kSecCSSigningInformation), &info) == errSecSuccess,
              let values = info as? [String: Any],
              let entitlements = values[kSecCodeInfoEntitlementsDict as String] as? [String: Any],
              (entitlements["beta-reports-active"] as? NSNumber) == NSNumber(value: true) else { return false }
        return true
    }
}
#endif
