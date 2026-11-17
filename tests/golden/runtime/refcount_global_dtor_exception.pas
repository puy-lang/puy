program RefcountGlobalDtorException;

uses System.SysUtils;

// The global variables are let go at the end of the isolate; an exception from a destructor
// then is reported like an uncaught exception of a `<fini>` and the exit code becomes 1
// (objects.md 8.5).

type
  TBad = class(TRefObject)
  public
    destructor Destroy; override;
  end;

var
  G: TBad;

destructor TBad.Destroy;
begin
  WriteLn('destroy');
  raise Exception.Create('from the destructor of a global');
end;

procedure Init;
begin
  G := TBad.Create;
end;

begin
  Init;
  WriteLn('end of the program');
end.
