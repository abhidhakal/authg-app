cask "authg" do
  version "1.0.4"

  if Hardware::CPU.arm?
    url "https://github.com/abhidhakal/authg-app/releases/download/v#{version}/AuthG_#{version}_aarch64.dmg",
        verified: "github.com/abhidhakal/authg-app/"
    sha256 "c06b176b4914c6b76a92908c97a1d55dba925e863f1ff274666693e6f8ea2732"
  else
    url "https://github.com/abhidhakal/authg-app/releases/download/v#{version}/AuthG_#{version}_x64.dmg",
        verified: "github.com/abhidhakal/authg-app/"
    sha256 "84373df2212f64c9a74a91b81462b0140b5888d4378cfc3532517a1dd467f91d"
  end

  name "AuthG"
  desc "Open-source, offline desktop 2FA authenticator for the menu bar"
  homepage "https://authg.abhinavdhakal.com"

  depends_on macos: ">= :catalina"

  app "AuthG.app"

  zap trash: [
    "~/Library/Application Support/com.authg.desktop",
    "~/Library/Caches/com.authg.desktop",
    "~/Library/Preferences/com.authg.desktop.plist",
    "~/Library/WebKit/com.authg.desktop",
  ]
end
