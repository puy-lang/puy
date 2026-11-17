program DtorExceptionPaths;

// Where a destructor's exception comes out, for the rows of the table in objects.md 8.5 without
// a case elsewhere: the instruction that let go of the last reference (`X := nil`); the implicit
// `finally` of a frame dropped by unwinding (the exception that was propagating goes into the new
// one's `Suppressed`); the cleanup after a constructor's exception (the constructor's exception
// goes into `Suppressed`); the release of a field (recorded, and the other fields are still
// released); and the release in the body of a `try..finally`.

uses
  System.SysUtils;

type
  TBad = class(TRefObject)
    Name: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
  end;

  TFailing = class(TRefObject)
    constructor Create;
    destructor Destroy; override;
  end;

  THolder = class(TRefObject)
    FA, FB: TBad;
    destructor Destroy; override;
  end;

constructor TBad.Create(const AName: string);
begin
  Name := AName;
end;

destructor TBad.Destroy;
begin
  WriteLn('destroy ', Name);
  raise Exception.Create('dtor ' + Name);
end;

constructor TFailing.Create;
begin
  raise Exception.Create('ctor');
end;

destructor TFailing.Destroy;
begin
  raise Exception.Create('dtor of failed');
end;

destructor THolder.Destroy;
begin
  WriteLn('destroy holder');
  inherited Destroy;
end;

// Creating through an `out` parameter leaves no temporary in the caller that would hold the
// instance until the caller ends
procedure Make(out X: TBad; const AName: string);
begin
  X := TBad.Create(AName);
end;

procedure MakeHolder(out H: THolder);
begin
  H := THolder.Create;
  Make(H.FA, 'fa');
  Make(H.FB, 'fb');
end;

procedure Show(E: Exception);
var
  I: Integer;
begin
  Write(E.Message);
  for I := 0 to Length(E.Suppressed) - 1 do
    Write(' [', E.Suppressed[I].Message, ']');
  WriteLn;
end;

procedure DropLocal;
var
  X: TBad;
begin
  X := TBad.Create('local');
  raise Exception.Create('propagating');
end;

procedure ReleaseInFinally;
var
  X: TBad;
begin
  X := TBad.Create('in finally');
  try
    WriteLn('body');
  finally
    X := nil;
  end;
end;

var
  X: TBad;
  F: TFailing;
  H: THolder;
begin
  Make(X, 'assigned');
  try
    X := nil;
  except
    on E: Exception do
      Show(E);
  end;
  try
    DropLocal;
  except
    on E: Exception do
      Show(E);
  end;
  F := nil;
  try
    F := TFailing.Create;
  except
    on E: Exception do
      Show(E);
  end;
  MakeHolder(H);
  try
    H := nil;
  except
    on E: Exception do
      Show(E);
  end;
  try
    ReleaseInFinally;
  except
    on E: Exception do
      Show(E);
  end;
end.
