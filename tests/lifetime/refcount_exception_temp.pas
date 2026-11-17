program RefcountExceptionTemp;

uses System.SysUtils;

// The temporary of a statement left by an exception keeps its instance until the routine ends
// (design/lifetime.md 3.4 A; the same time as Delphi): the instance made for the `const`
// argument is destroyed after the handler.

type
  TNote = class(TRefObject)
  public
    destructor Destroy; override;
  end;

destructor TNote.Destroy;
begin
  WriteLn('destroy');
  inherited Destroy;
end;

procedure Use(const N: TNote; Fail: Boolean);
begin
  if Fail then
    raise Exception.Create('fail');
end;

procedure Run;
begin
  try
    Use(TNote.Create, True);
  except
    on E: Exception do
      WriteLn('caught ', E.Message);
  end;
  WriteLn('after the handler');
end;

begin
  Run;
  WriteLn('end');
end.
