program HaltDtorException;

// `Halt` unwinding through an implicit `finally` that lets go of the last reference to a
// reference counted instance (objects.md 8.5): the destructor runs, and its exception is reported
// like an exception in a `finally` during `Halt` (exceptions.md 5.3); the exit code is `Halt`'s.

uses
  System.SysUtils;

type
  TBad = class(TRefObject)
    destructor Destroy; override;
  end;

destructor TBad.Destroy;
begin
  WriteLn('destroy');
  raise Exception.Create('dtor during Halt');
end;

procedure Make(out X: TBad);
begin
  X := TBad.Create;
end;

procedure Run;
var
  X: TBad;
begin
  Make(X);
  Halt(4);
end;

begin
  Run;
  WriteLn('not reached');
end.
