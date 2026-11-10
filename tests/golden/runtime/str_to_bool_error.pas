program StrToBoolError;

// StrToBool of text that is neither True / False nor an integer is EConvertError
// (semantics.md 5.3).

uses System.SysUtils;

begin
  WriteLn(StrToBool('false'));
  WriteLn(StrToBool('maybe'));
end.
