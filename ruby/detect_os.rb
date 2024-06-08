module DetectOS
  def self.windows_arm?
    return unless !(/cygwin|mswin|mingw|bccwin|wince|emx/ =~ RbConfig::CONFIG['arch']).nil? && !(/arm64/ =~ RbConfig::CONFIG['arch']).nil?
    true
  end

  def self.windows?
    return if (/cygwin|mswin|mingw|bccwin|wince|emx/ =~ RbConfig::CONFIG['arch']).nil?
    true
  end

  def self.mac_arm?
    return unless !(/darwin/ =~ RbConfig::CONFIG['arch']).nil? && !(/arm64/ =~ RbConfig::CONFIG['arch']).nil?
    true
  end

  def self.mac?
    return unless !(/darwin/ =~ RbConfig::CONFIG['arch']).nil? && !(/x86_64/ =~ RbConfig::CONFIG['arch']).nil?
    true
  end

  def self.linux_arm_musl?
    return unless !(/linux/ =~ RbConfig::CONFIG['arch']).nil? && !(/aarch64/ =~ RbConfig::CONFIG['arch']).nil? && !(/musl/ =~ RbConfig::CONFIG['arch']).nil?
    true
  end

  def self.linux_musl?
    return unless !(/linux/ =~ RbConfig::CONFIG['arch']).nil? && !(/x86_64/ =~ RbConfig::CONFIG['arch']).nil?&& !(/musl/ =~ RbConfig::CONFIG['arch']).nil?
    true
  end
  def self.linux_arm?
    return unless !(/linux/ =~ RbConfig::CONFIG['arch']).nil? && !(/aarch64/ =~ RbConfig::CONFIG['arch']).nil?
    true
  end

  def self.linux?
    return unless !(/linux/ =~ RbConfig::CONFIG['arch']).nil? && !(/x86_64/ =~ RbConfig::CONFIG['arch']).nil?
    true
  end

  def self.debug?
    return if ENV['DEBUG_TARGET'].nil?
    true
  end

  def self.get_bin_path
    if debug?
      ENV['PACT_FFI_LIBRARY_PATH'].to_s
    elsif windows_arm?
      File.expand_path("#{__dir__}/../rust/target/aarch64-pc-windows-msvc/release/pact_ffi.dll")
    elsif windows?
      File.expand_path("#{__dir__}/../rust/target/x86_64-pc-windows-msvc/release/pact_ffi.dll")
    elsif mac_arm?
      File.expand_path("#{__dir__}/../rust/target/aarch64-apple-darwin/release/libpact_ffi.dylib")
    elsif mac?
      File.expand_path("#{__dir__}/../rust/target/x86_64-apple-darwin/release/libpact_ffi.dylib")
    elsif linux_arm_musl?
      File.expand_path("#{__dir__}/../rust/target/aarch64-unknown-linux-musl/release/libpact_ffi.so")
    elsif linux_musl?
      File.expand_path("#{__dir__}/../rust/target/x86_64-unknown-linux-musl/release/libpact_ffi.so")
    elsif linux_arm?
      File.expand_path("#{__dir__}/../rust/target/aarch64-unknown-linux-gnu/release/libpact_ffi.so")
    elsif linux?
      File.expand_path("#{__dir__}/../rust/target/x86_64-unknown-linux-gnu/release/libpact_ffi.so")
    else
      raise "Detected #{RbConfig::CONFIG['arch']}-- I have no idea what to do with that."
    end
  end

  def self.get_os
    if windows_arm?
      'win-arm64'
    elsif windows?
      'win'
    elsif mac_arm?
      'macos-arm64'
    elsif mac?
      'linux-x8664'
    elsif linux_arm?
      'linux-aarch64'
    elsif linux?
      'linux-x8664'
    else
      raise "Detected #{RbConfig::CONFIG['arch']}-- I have no idea what to do with that."
    end
  end
end

ENV['PACT_DEBUG'] ? (puts "Detected platform: #{RbConfig::CONFIG['arch']} \nLoad Path: #{DetectOS.get_bin_path}" ): nil