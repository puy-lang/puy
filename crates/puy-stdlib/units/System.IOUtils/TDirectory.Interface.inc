type
  // Whether TDirectory's listings go into the subdirectories.
  TSearchOption = (soTopDirectoryOnly, soAllDirectories);

  // Directories (docs/spec/stdlib/System.IOUtils/TDirectory.md). The listings are sorted by
  // name (the order of the code points) on every target, a directory's entries before those of
  // its subdirectories; a search pattern has `*` (any run of characters) and `?` (one), and
  // compares letters as the target's file names do (Windows ignores case). A missing directory
  // is EDirectoryNotFoundException, an empty path EArgumentException.
  TDirectory = record
  public
    class function Exists(const Path: string; FollowLink: Boolean = True): Boolean; static;
    // Creates the directory and its missing parents; nothing if it exists.
    class procedure CreateDirectory(const Path: string); static;
    // Removes an empty directory; with Recursive, everything in it first (links are removed,
    // not followed).
    class procedure Delete(const Path: string); overload; static;
    class procedure Delete(const Path: string; const Recursive: Boolean); overload; static;
    // Moves (renames) a directory; EInOutError if DestDirName exists.
    class procedure Move(const SourceDirName, DestDirName: string); static;
    // Copies a directory with everything in it to DestDirName (created; files there are
    // replaced). A link to a file is copied as the file; a link to a directory is EInOutError.
    class procedure Copy(const SourceDirName, DestDirName: string); static;
    class function IsEmpty(const Path: string): Boolean; static;
    // The paths (Path joined with each name) of the files, the directories, or both.
    class function GetFiles(const Path: string): TArray<string>; overload; static;
    class function GetFiles(const Path: string;
      const SearchOption: TSearchOption): TArray<string>; overload; static;
    class function GetFiles(const Path, SearchPattern: string): TArray<string>; overload; static;
    class function GetFiles(const Path, SearchPattern: string;
      const SearchOption: TSearchOption): TArray<string>; overload; static;
    class function GetDirectories(const Path: string): TArray<string>; overload; static;
    class function GetDirectories(const Path: string;
      const SearchOption: TSearchOption): TArray<string>; overload; static;
    class function GetDirectories(const Path, SearchPattern: string): TArray<string>; overload;
      static;
    class function GetDirectories(const Path, SearchPattern: string;
      const SearchOption: TSearchOption): TArray<string>; overload; static;
    class function GetFileSystemEntries(const Path: string): TArray<string>; overload; static;
    class function GetFileSystemEntries(const Path: string;
      const SearchOption: TSearchOption): TArray<string>; overload; static;
    class function GetFileSystemEntries(const Path, SearchPattern: string): TArray<string>;
      overload; static;
    class function GetFileSystemEntries(const Path, SearchPattern: string;
      const SearchOption: TSearchOption): TArray<string>; overload; static;
    // The working directory (System.SysUtils's GetCurrentDir / SetCurrentDir; setting raises
    // EDirectoryNotFoundException for a missing directory and EInOutError for other failures).
    class function GetCurrentDirectory: string; static;
    class procedure SetCurrentDirectory(const Path: string); static;
  end;
