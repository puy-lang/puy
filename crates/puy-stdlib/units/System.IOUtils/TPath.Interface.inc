type
  // Paths as strings (docs/spec/stdlib/System.IOUtils/TPath.md). The separators are the
  // target's, found at run time: on Windows `\` and `/` separate the parts and `:` ends a
  // drive; elsewhere only `/` separates. Nothing here asks the file system, except the
  // directories of the host (GetTempPath, GetHomePath) and the working directory (GetFullPath).
  TPath = record
  public
    // '\' on Windows, '/' elsewhere.
    class function DirectorySeparatorChar: Char; static;
    // '/' on every target (Windows accepts both).
    class function AltDirectorySeparatorChar: Char; static;
    // ':' on Windows, '/' elsewhere (where there are no drives).
    class function VolumeSeparatorChar: Char; static;
    // The separator of the paths of a list (PATH): ';' on Windows, ':' elsewhere.
    class function PathSeparator: Char; static;
    // '.'.
    class function ExtensionSeparatorChar: Char; static;
    // Path2 after Path1 with one separator between; Path2 alone if it is rooted or Path1 is
    // empty, Path1 alone if Path2 is empty.
    class function Combine(const Path1, Path2: string): string; overload; static;
    class function Combine(const Path1, Path2, Path3: string): string; overload; static;
    // The directory part without its trailing separator (as ExtractFileDir); '' if none.
    class function GetDirectoryName(const FileName: string): string; static;
    // The name after the last separator.
    class function GetFileName(const FileName: string): string; static;
    // The name without its extension.
    class function GetFileNameWithoutExtension(const FileName: string): string; static;
    // The extension of the name with its '.' (`.gz` of `a.tar.gz`, `.` of `a.`); '' if none.
    class function GetExtension(const FileName: string): string; static;
    // The path with its extension replaced by Extension (a '.' is put before it if missing).
    class function ChangeExtension(const Path, Extension: string): string; static;
    // Whether the name has an extension (GetExtension is not '').
    class function HasExtension(const Path: string): Boolean; static;
    // Whether Path starts at a root: a separator, or on Windows a drive (`C:`, `\\server`).
    class function IsPathRooted(const Path: string): Boolean; static;
    // The root Path starts at (`/`; `C:\`, `C:`, `\`, `\\server\share` on Windows); '' if
    // none.
    class function GetPathRoot(const Path: string): string; static;
    // The absolute path (ExpandFileName): joined to the working directory, `.` and `..`
    // resolved by name.
    class function GetFullPath(const Path: string): string; static;
    // The directory for temporary files and the home directory of the user, as the host gives
    // them; '' where there is none.
    class function GetTempPath: string; static;
    class function GetHomePath: string; static;
  end;
