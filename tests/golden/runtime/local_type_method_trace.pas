program LocalTypeMethodTrace;

// The stack trace names a method of a local type after the routine declaring the type
// (`B.TLoc.S`, semantics.md 4.9), without the overload number of a same-named method in `A`.

uses System.SysUtils;

procedure A;
type
  TLoc = record
    procedure S;
  end;
procedure TLoc.S;
begin
  WriteLn('A');
end;
var
  L: TLoc = Default(TLoc);
begin
  L.S;
end;

procedure B;
type
  TLoc = record
    procedure S;
  end;
procedure TLoc.S;
begin
  raise Exception.Create('from B');
end;
var
  L: TLoc = Default(TLoc);
begin
  L.S;
end;

begin
  A;
  B;
end.
