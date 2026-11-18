import Foundation

#if FEI_CLOUD_PRODUCT
let containerID = "iCloud.app.feinotes.mac"
let cloudEnvironment = "Production"
#else
let containerID = "iCloud.app.quietmd.cloudlab"
let cloudEnvironment = "Development"
#endif

@_cdecl("fei_ios_product_cloud")
func feiIOSProductCloud() -> Bool { cloudEnvironment == "Production" }

@_cdecl("fei_ios_product_value")
func feiIOSProductValue(_ key: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>? {
    guard let value = Bundle.main.object(forInfoDictionaryKey: String(cString: key)) as? String else { return nil }
    return strdup(value)
}
