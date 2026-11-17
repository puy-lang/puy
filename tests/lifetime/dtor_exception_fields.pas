// puy-test: memory-limit=1m
program DtorExceptionFields;

uses System.SysUtils;

// The managed fields of an instance are let go even when its destruction or its construction
// fails (objects.md 3.3, 8.5: steps 4 and 5 always run): a destructor that raises, a
// `BeforeDestruction` that raises, and a constructor that raises after filling the fields. The
// loop would run out of the 1 MB limit if anything were kept, and nothing is left at the end.

type
  TInfo = record
    Title: string;
    Tags: array of string;
  end;

  TBase = class
  public
    Name: string;
    Info: TInfo;
    Mode: Integer;
    constructor Create(AMode: Integer);
    destructor Destroy; override;
    procedure BeforeDestruction; override;
  end;

  TDoc = class(TBase)
  public
    Lines: array of string;
    destructor Destroy; override;
  end;

constructor TBase.Create(AMode: Integer);
begin
  Mode := AMode;
  Name := StringOfChar('n', 100);
  Info.Title := StringOfChar('t', 80);
  SetLength(Info.Tags, 2);
  Info.Tags[1] := StringOfChar('g', 60);
  if Mode = 2 then
    raise Exception.Create('constructor failed');
end;

destructor TBase.Destroy;
begin
  if Mode = 1 then
    raise Exception.Create('TBase.Destroy failed');
  inherited;
end;

procedure TBase.BeforeDestruction;
begin
  if Mode = 3 then
    raise Exception.Create('BeforeDestruction failed');
end;

destructor TDoc.Destroy;
begin
  if Mode = 0 then
    raise Exception.Create('TDoc.Destroy failed');
  inherited;
end;

var
  D: TDoc;
  I, J, Caught: Integer;
begin
  Caught := 0;
  for I := 1 to 2000 do
  begin
    try
      D := TDoc.Create(I mod 4);
      SetLength(D.Lines, 4);
      for J := 0 to 3 do
        D.Lines[J] := StringOfChar('l', 50) + IntToStr(J);
      D.Free;
    except
      on E: Exception do
        Inc(Caught);
    end;
  end;
  WriteLn(Caught);
end.
