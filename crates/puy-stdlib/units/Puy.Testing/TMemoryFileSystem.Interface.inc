// ===== The in-memory file system (docs/spec/stdlib/Puy.Testing/README.md chapter 7) =====

type
  TMemoryFileSystem = class sealed(TRefObject, IFileSystem)
  private
    type
      TNode = class(TRefObject)
      public
        Kind: TFileKind;
        Children: TDictionary<string, TNode>;
        Bytes: TArray<Byte>;
        ReadOnly: Boolean;
        Created: Int64;
        Accessed: Int64;
        Modified: Int64;
        constructor Create(AKind: TFileKind);
        destructor Destroy; override;
        procedure CheckWritable(const Path: string);
        function FindChild(const Name: string): TNode;
        procedure AddChild(const Name: string; Node: TNode; const AtTime: Int64);
      end;
  private
    FRoot: TNode;
    FWorkingDirectory: string;
    function PathParts(const Path: string): TArray<string>;
    function DirectoryAt(const Parts: TArray<string>; Count: Integer;
      const Path: string; CreateMissing: Boolean = False): TNode;
    function NodeAt(const Parts: TArray<string>; const Path: string): TNode;
    function IsWithin(const Parts, Directory: TArray<string>): Boolean;
    procedure Init(const WorkingDirectory: string);
  public
    constructor Create; overload;
    constructor Create(const WorkingDirectory: string); overload;
    function OpenFile(const Path: string; Mode: TFileOpenMode): IFileHandle;
    function GetInfo(const Path: string; FollowLink: Boolean): TFileSystemInfo;
    function ListDirectory(const Path: string): TArray<TFileSystemEntry>;
    procedure CreateDirectory(const Path: string; Recursive: Boolean);
    procedure RemoveDirectory(const Path: string);
    procedure RemoveFile(const Path: string);
    procedure Rename(const OldPath, NewPath: string);
    procedure CopyFile(const Source, Dest: string);
    procedure SetTime(const Path: string; Kind: TFileTimeKind; const Nanos: Int64);
    procedure SetReadOnly(const Path: string; ReadOnly: Boolean);
    function Canonicalize(const Path: string): string;
  end;
