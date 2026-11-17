program DtorHalt;

// `Halt` in a destructor (objects.md 8.5): the destruction ends without the parents'
// destructors (the parent's `TBase.Destroy` does not run), the instance's fields are let go,
// and `Halt` goes on through the `finally` blocks with the exit code it was given.

type
  TBase = class
  public
    Name: string;
    destructor Destroy; override;
  end;

  TStopper = class(TBase)
  public
    destructor Destroy; override;
  end;

destructor TBase.Destroy;
begin
  WriteLn('TBase.Destroy');
  inherited;
end;

destructor TStopper.Destroy;
begin
  WriteLn('TStopper.Destroy ', Name);
  Halt(3);
end;

var
  S: TStopper;
begin
  S := TStopper.Create;
  S.Name := 'stopper';
  try
    S.Free;
    WriteLn('not reached');
  finally
    WriteLn('finally');
  end;
end.
