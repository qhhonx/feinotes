cask "quietmd" do
  arch arm: "arm64", intel: "x86_64"

  version "0.1.13"
  sha256 arm:   "9a224194402f6fbc6d7864e4962878d4fad57d24ea855bdc0a37d3f1aaa8167a",
         intel: "09c3287d58a36c89ddfb1030cce0abb627ec5f0c0996e2d8d49cb60bf72f12fd"

  # The zip, not the DMG people download: it is the package the in-app updater installs (auto_updates),
  # so brew and the app swap in byte-identical bundles, and unpacking it needs no hdiutil mount.
  url "https://github.com/s1dashu/quiet-md/releases/download/v#{version}/quiet-md-#{version}-macos-#{arch}.zip"
  name "Quiet.md"
  desc "Local-first editor for Markdown notes, documents, tables and boards"
  homepage "https://quiet.md/"

  livecheck do
    url :url
    strategy :github_latest
  end

  auto_updates true
  depends_on macos: :ventura

  app "quiet.md.app"

  # The library (~/Documents/Quiet.md by default) holds the user's notes and is never removed.
  zap trash: [
    "~/Library/Application Support/quiet-md",
    "~/Library/Caches/com.s1dashu.quietmd",
    "~/Library/Caches/quiet-md",
    "~/Library/Caches/quiet_md",
    "~/Library/Logs/quiet.md",
    "~/Library/Preferences/com.s1dashu.quietmd.plist",
  ]
end
