// puy-test: memory-limit=1m
program ClassFreeFields;

uses System.SysUtils;

// `Free` and `Destroy` let go of the managed fields of an instance, its own and its parent's
// (strings, dynamic arrays, records holding strings; objects.md 3.3): the loop would run out of
// the 1 MB limit if anything were kept, and nothing is left at the end. A field holding another
// instance of a class managed by hand is not freed with it (3.3, step 4).

type
  TInfo = record
    Title: string;
    Tags: array of string;
  end;
  TBase = class
    Name: string;
  end;
  TDoc = class(TBase)
    Lines: array of string;
    Info: TInfo;
    Child: TDoc;
  end;

var
  D, C: TDoc;
  I, J: Integer;
begin
  for I := 1 to 2000 do
  begin
    D := TDoc.Create;
    D.Name := StringOfChar('n', 100) + IntToStr(I);
    SetLength(D.Lines, 4);
    for J := 0 to 3 do
      D.Lines[J] := StringOfChar('l', 50) + IntToStr(J);
    D.Info.Title := StringOfChar('t', 80);
    SetLength(D.Info.Tags, 2);
    D.Info.Tags[1] := StringOfChar('g', 60);
    C := TDoc.Create;
    C.Name := StringOfChar('c', 100);
    D.Child := C;
    if Odd(I) then
      D.Free
    else
      D.Destroy;
    C.Free;
  end;
  WriteLn('done');
end.
