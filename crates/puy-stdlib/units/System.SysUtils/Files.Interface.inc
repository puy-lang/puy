// Files, directories and file names (docs/spec/stdlib/System.SysUtils/Files.md). The
// separators of file names are the target's, found at run time: on Windows `\` and `/` separate
// the parts and `:` ends a drive; elsewhere only `/` separates.

type
  // A file (EFileNotFoundException) or a directory (EDirectoryNotFoundException) that
  // System.IOUtils's TFile and TDirectory did not find. Children of EInOutError (Puy only;
  // Delphi's are not), with the ErrorKind ekNotFound and the ErrorCode of the host.
  EFileNotFoundException = class(EInOutError);
  EDirectoryNotFoundException = class(EInOutError);

// The separator of the parts of a path: '\' on Windows, '/' elsewhere.
function PathDelim: string;
// The end of a drive: ':' on Windows, '' elsewhere.
function DriveDelim: string;
// The separator of the paths of a list (PATH): ';' on Windows, ':' elsewhere.
function PathSep: string;

// The position of the last character of S that is one of Delimiters; 0 if none.
function LastDelimiter(const Delimiters, S: string): Integer;
// Whether S[Index] separates the parts of a path.
function IsPathDelimiter(const S: string; Index: Integer): Boolean;
function IncludeTrailingPathDelimiter(const S: string): string;
function ExcludeTrailingPathDelimiter(const S: string): string;
// The drive (`C:`, `\\server\share`; '' where there are no drives), the directory with its
// trailing separator, the directory without it, the name, the extension with its '.'.
function ExtractFileDrive(const FileName: string): string;
function ExtractFilePath(const FileName: string): string;
function ExtractFileDir(const FileName: string): string;
function ExtractFileName(const FileName: string): string;
function ExtractFileExt(const FileName: string): string;
function ChangeFileExt(const FileName, Extension: string): string;
function ChangeFilePath(const FileName, Path: string): string;
// The absolute path of FileName (joined to the working directory), with `.` and `..` resolved
// by name; the file system is not asked.
function ExpandFileName(const FileName: string): string;

// The file system of the host. Without it (a target without files, an embedding that did not
// grant it) each raises ENotSupportedException; the other failures are False.
function FileExists(const FileName: string; FollowLink: Boolean = True): Boolean;
function DirectoryExists(const Directory: string; FollowLink: Boolean = True): Boolean;
function DeleteFile(const FileName: string): Boolean;
function RenameFile(const OldName, NewName: string): Boolean;
function CreateDir(const Dir: string): Boolean;
function ForceDirectories(const Dir: string): Boolean;
function RemoveDir(const Dir: string): Boolean;

// The working directory: one per process, seen by every isolate. '/' when the host gives no
// OS information.
function GetCurrentDir: string;
function SetCurrentDir(const Dir: string): Boolean;

const
  // How TFileStream (System.Classes) opens a file: one of the three ways, `or` a share mode.
  // Puy does not lock files: the share modes are accepted and have no effect
  // (docs/spec/stdlib/System.Classes/TStream.md).
  fmOpenRead = $0000;
  fmOpenWrite = $0001;
  fmOpenReadWrite = $0002;
  fmShareCompat = $0000;
  fmShareExclusive = $0010;
  fmShareDenyWrite = $0020;
  fmShareDenyRead = $0030;
  fmShareDenyNone = $0040;

// The file system a program installs in place of the host's (docs/spec/stdlib/System.SysUtils/
// FileSystem.md; Puy only). While one is installed, the file functions of System.SysUtils,
// TFileStream (System.Classes), System.IOUtils and TFilePath (Puy.Paths) call it instead of the
// host's file system: they look at TFileSystem.Installed before each call of their Rust parts,
// which only know the host. In System.SysUtils, not Puy.*, because those System.* units use it
// (requirements §8.1).

type
  // What a path names.
  TFileKind = (fkFile, fkDirectory, fkSymLink, fkOther);

  // The attributes of a path (IFileSystem.GetInfo). The times are nanoseconds since the UNIX
  // epoch (UTC); a time the file system does not keep has its Has... False.
  TFileSystemInfo = record
    Kind: TFileKind;
    Size: Int64;
    ReadOnly: Boolean;
    HasModified: Boolean;
    Modified: Int64;
    HasAccessed: Boolean;
    Accessed: Int64;
    HasCreated: Boolean;
    Created: Int64;
  end;

  // One entry of a directory (IFileSystem.ListDirectory).
  TFileSystemEntry = record
    Name: string;
    Kind: TFileKind;
  end;

  // How IFileSystem.OpenFile opens a file: to read, to write or both (the file must exist), or
  // to create it (or cut it to 0 bytes) to read and write.
  TFileOpenMode = (omRead, omWrite, omReadWrite, omCreate);

  // Where IFileHandle.Seek counts from.
  TFileSeekOrigin = (fsoBeginning, fsoCurrent, fsoEnd);

  // Which time IFileSystem.SetTime sets.
  TFileTimeKind = (ftCreation, ftLastAccess, ftLastWrite);

  // A file an IFileSystem opened. A failure is an EInOutError with its ErrorKind (as the host's
  // failures are); the callers turn it into their own exceptions as they do the host's.
  IFileHandle = interface
    // Up to Count bytes from the position (fewer at the end of the file; none past it).
    function Read(Count: Integer): TArray<Byte>;
    // Writes the Count bytes of Bytes at Offset (the range is within Bytes).
    procedure Write(const Bytes: TArray<Byte>; Offset, Count: Integer);
    // Moves the position and returns it (never before the start: the callers check that).
    function Seek(const Offset: Int64; Origin: TFileSeekOrigin): Int64;
    function GetSize: Int64;
    // Cuts or extends the file to Size bytes (extended with zeros); the position stays.
    procedure SetSize(const Size: Int64);
    procedure Flush;
    // Called once when the stream on the file is closed.
    procedure Close;
  end;

  // A file system that replaces the host's (TFileSystem.Install). Each method is one operation
  // of the host's file system; a failure is an EInOutError with its ErrorKind (ekNotFound for a
  // missing path, ekAlreadyExists, ...), which the callers handle as the host's. Paths are
  // passed as the program wrote them (relative ones are not expanded). While it is installed,
  // the file functions called from inside it come back to it too. Use TFileSystem.Host to
  // delegate an operation to the host explicitly.
  IFileSystem = interface
    function OpenFile(const Path: string; Mode: TFileOpenMode): IFileHandle;
    // With FollowLink, the attributes of what a symbolic link points to.
    function GetInfo(const Path: string; FollowLink: Boolean): TFileSystemInfo;
    // The entries of a directory, in any order and without `.` and `..`.
    function ListDirectory(const Path: string): TArray<TFileSystemEntry>;
    // With Recursive, the missing parents too, and an existing directory is not an error.
    procedure CreateDirectory(const Path: string; Recursive: Boolean);
    // Removes an empty directory.
    procedure RemoveDirectory(const Path: string);
    // Removes a file (or a symbolic link itself).
    procedure RemoveFile(const Path: string);
    procedure Rename(const OldPath, NewPath: string);
    // Copies the contents of a file, replacing Dest.
    procedure CopyFile(const Source, Dest: string);
    procedure SetTime(const Path: string; Kind: TFileTimeKind; const Nanos: Int64);
    procedure SetReadOnly(const Path: string; ReadOnly: Boolean);
    // The absolute path with every symbolic link, `.` and `..` resolved.
    function Canonicalize(const Path: string): string;
  end;

  // The isolate's file system. Installing one replaces the host's until Uninstall (or the end
  // of the isolate); a new isolate starts with the host's.
  TFileSystem = class
  protected
    class function WrapHostFile(const Handle: TFileHandle): IFileHandle; static;
    // The kind numbers used at the native boundary (1 a file, 2 a directory, ...).
    class function NativeOfFileKind(Kind: TFileKind): Integer; static;
  public
    class procedure Install(const FS: IFileSystem); static;
    class procedure Uninstall; static;
    // The installed file system, or nil while the host's is used.
    class function Installed: IFileSystem; static;
    // The underlying host file system, independent of the installed file system.
    class function Host: IFileSystem; static;
  end;
