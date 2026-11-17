program RefcountRetDtorException;

uses System.SysUtils;

// An exception from a destructor that the implicit `finally` of `Ret` runs is raised at the
// caller's `Call` (objects.md 8.5): the trace has the destructor, then the line of the call.

type
  TBad = class(TRefObject)
  public
    destructor Destroy; override;
  end;

destructor TBad.Destroy;
begin
  raise Exception.Create('from the destructor');
end;

procedure Run;
var
  B: TBad;
begin
  B := TBad.Create;
  WriteLn(Assigned(B));
end;

begin
  WriteLn('before');
  Run;
  WriteLn('not reached');
end.
