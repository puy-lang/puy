unit System.SysUtils;

// The part of System.SysUtils written in Pascal. The rest of the unit (IntToStr, Format,
// TReplaceFlags, the built-in exception classes, ...) is built into the compiler; a `uses`
// of the unit makes both visible (docs/spec/collections.md 1.1, 3.2).
//
// Types with files of their own are split as in System.Generics.Collections (Name.Interface.inc,
// .Implementation.inc, .Finalization.inc), read only when used (docs/spec/collections.md 1.2).

interface
uses System.Classes.Native;
type
  // Raised by the containers of System.Generics.Collections: a duplicate key, an empty stack
  // or queue.
  EListError = class(Exception);

  // A key that is not in a dictionary (Puy only; caught by `on E: EListError` too).
  EKeyNotFound = class(EListError);

{$I System.SysUtils/TEncoding.Interface.inc}

{$I System.SysUtils/TOSVersion.Interface.inc}

{$I System.SysUtils/Files.Interface.inc}

{$I System.SysUtils/DateTime.Interface.inc}

{$I System.SysUtils/IfThen.Interface.inc}

implementation

uses
  // The Rust parts of the unit (crates/puy-stdlib-ext; docs/design/stdlib.md 3.1)
  System.SysUtils.Native, System.IOUtils.Native, System.FileSystem.Native;

{$I System.SysUtils/TEncoding.Implementation.inc}

{$I System.SysUtils/TOSVersion.Implementation.inc}

{$I System.SysUtils/Files.Implementation.inc}

{$I System.SysUtils/DateTime.Implementation.inc}

{$I System.SysUtils/IfThen.Implementation.inc}

finalization

{$I System.SysUtils/TEncoding.Finalization.inc}

end.
