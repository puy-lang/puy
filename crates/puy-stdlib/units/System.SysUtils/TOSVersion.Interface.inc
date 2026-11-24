type
  // The OS the program runs on (docs/spec/stdlib/System.SysUtils/TOSVersion.md), read from the
  // host the first time a member is used. The bytecode is the same on every target, so a
  // program checks the platform at run time with these instead of conditional symbols.
  TOSVersion = record
  public type
    // The CPU architecture (arWasm32 and arOther are Puy's).
    TArchitecture = (arIntelX86, arIntelX64, arARM32, arARM64, arWasm32, arOther);
    // The kind of OS (pfWasi, pfBrowser and pfEmbedded are Puy's). pfEmbedded is an embedding
    // that gives no OS information, or an OS Puy has no name for.
    TPlatform = (pfWindows, pfMacOS, pfiOS, pfAndroid, pfWinRT, pfLinux, pfWasi, pfBrowser,
      pfEmbedded);
  private
    class var FLoaded: Boolean;
    class var FArchitecture: TArchitecture;
    class var FPlatform: TPlatform;
    class var FArchName: string;
    class var FName: string;
    class var FMajor: Integer;
    class var FMinor: Integer;
    class var FBuild: Integer;
    class procedure Load; static;
    class function GetArchitecture: TArchitecture; static;
    class function GetPlatform: TPlatform; static;
    class function GetName: string; static;
    class function GetMajor: Integer; static;
    class function GetMinor: Integer; static;
    class function GetBuild: Integer; static;
    class function GetServicePackMajor: Integer; static;
    class function GetServicePackMinor: Integer; static;
  public
    class property Architecture: TArchitecture read GetArchitecture;
    class property Platform: TPlatform read GetPlatform;
    class property Name: string read GetName;
    class property Major: Integer read GetMajor;
    class property Minor: Integer read GetMinor;
    class property Build: Integer read GetBuild;
    // Always 0: no OS Puy runs on has service packs.
    class property ServicePackMajor: Integer read GetServicePackMajor;
    class property ServicePackMinor: Integer read GetServicePackMinor;
    // Whether the version is at least the one given.
    class function Check(AMajor: Integer): Boolean; overload; static;
    class function Check(AMajor, AMinor: Integer): Boolean; overload; static;
    class function Check(AMajor, AMinor, AServicePackMajor: Integer): Boolean; overload; static;
    // `Name Major.Minor.Build (architecture)`.
    class function ToString: string; static;
  end;
