// puy-test: memory-limit=1m
program DestroyWithoutDtor;

uses System.SysUtils;

// Classes with no destructor or BeforeDestruction of their own are destroyed by `Free` and by
// the clean-up after a constructor's exception without a frame for the destruction procedure
// (objects.md 14.8): their managed fields are still let go (the loop would run out of the 1 MB
// limit otherwise), for a class managed by hand and a reference counted one. A field holding a
// reference counted instance whose destructor raises makes the exception come out at the `Free`
// that let go of it (caught there, and the trace of the uncaught one shows that line). `Fill`
// sets the field so that no temporary of the caller holds the instance (objects.md 4.1).

type
  TPlain = class
    Name: string;
    Lines: array of string;
    constructor Create(Fail: Boolean);
  end;

  TCounted = class(TRefObject)
    Name: string;
    Lines: array of string;
    constructor Create(Fail: Boolean);
  end;

  TNoisy = class(TRefObject)
    destructor Destroy; override;
  end;

  THolder = class
    Item: TNoisy;
  end;

constructor TPlain.Create(Fail: Boolean);
begin
  Name := StringOfChar('p', 100);
  SetLength(Lines, 3);
  Lines[2] := StringOfChar('l', 80);
  if Fail then
    raise Exception.Create('TPlain failed');
end;

constructor TCounted.Create(Fail: Boolean);
begin
  Name := StringOfChar('c', 100);
  SetLength(Lines, 3);
  Lines[2] := StringOfChar('l', 80);
  if Fail then
    raise Exception.Create('TCounted failed');
end;

destructor TNoisy.Destroy;
begin
  raise Exception.Create('TNoisy.Destroy failed');
end;

procedure Fill(H: THolder);
begin
  H.Item := TNoisy.Create;
end;

var
  P: TPlain;
  C: TCounted;
  H: THolder;
  I, Caught: Integer;
begin
  Caught := 0;
  for I := 1 to 2000 do
  begin
    try
      P := TPlain.Create(Odd(I));
      P.Free;
    except
      on E: Exception do
        Inc(Caught);
    end;
    try
      C := TCounted.Create(not Odd(I));
      C := nil;
    except
      on E: Exception do
        Inc(Caught);
    end;
  end;
  WriteLn(Caught);
  H := THolder.Create;
  Fill(H);
  try
    H.Free;
    WriteLn('not reached');
  except
    on E: Exception do
      WriteLn('caught ', E.Message);
  end;
  H := THolder.Create;
  Fill(H);
  H.Free;
  WriteLn('not reached');
end.
