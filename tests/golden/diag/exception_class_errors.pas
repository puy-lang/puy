program ExceptionClassErrors;
// Diagnostics of exceptions as classes (objects.md 8.3, 16.1): `raise` of a value whose class is
// not an exception class is E3021 (a class, also exactly `TObject` even if it holds an exception);
// `on E: T` with a class that is not an exception class is E0733 with a note (`TObject` too); a
// value that is not a class stays E0702; `ExceptAddr` is a removed name (E3004).

uses System.SysUtils;

type
  TPlain = class
  end;

  TCounted = class(TRefObject)
  end;

var
  O: TObject;
  P: TPlain;
  C: TCounted;
begin
  O := nil;
  P := nil;
  C := nil;
  if O = nil then
    raise O;
  if P = nil then
    raise P;
  if C = nil then
    raise C;
  if C = nil then
    raise 'text';
  try
    WriteLn;
  except
    on E: TObject do
      WriteLn;
    on E: TPlain do
      WriteLn;
  end;
  O := ExceptAddr;
end.
