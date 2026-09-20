cask "authg" do
  version "1.0.6"

  if Hardware::CPU.arm?
    url "https://github.com/abhidhakal/authg-app/releases/download/v#{version}/AuthG_#{version}_aarch64.dmg"
    sha256 "63a2aae47318e3f412c28042de8b32d06d5b22b6b678daeaa8a18404c6c25244"
  else
    url "https://github.com/abhidhakal/authg-app/releases/download/v#{version}/AuthG_#{version}_x64.dmg"
    sha256 "9461afc7fb4fca75112b0ca23bac414cd435e4ae538c488ec45b18853f5f3ea0"
  end

  name "AuthG"
  desc "Open-source, offline desktop 2FA authenticator for the menu bar"
  homepage "https://authg.abhinavdhakal.com"

  depends_on macos: ">= :big_sur"

  app "AuthG.app"

  zap trash: [
    "~/Library/Application Support/com.authg.desktop",
    "~/Library/Caches/com.authg.desktop",
    "~/Library/Preferences/com.authg.desktop.plist",
    "~/Library/WebKit/com.authg.desktop",
  ]
end
