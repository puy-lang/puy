program RefcountDtorSuppressed;

uses System.SysUtils;

// When the implicit `finally` of a routine destroys two locals whose destructors both raise,
// the first exception goes to the `Suppressed` of the second, which propagates
// (objects.md 8.5).

type
  TBad = class(TRefObject)
  public
    Name: string;
    destructor Destroy; override;
  end;

destructor TBad.Destroy;
begin
  WriteLn('destroy ', Name);
  raise Exception.Create('from ' + Name);
end;

procedure Run;
var
  A, B: TBad;
begin
  A := TBad.Create;
  A.Name := 'A';
  B := TBad.Create;
  B.Name := 'B';
  WriteLn(A.Name, B.Name);
end;

begin
  try
    Run;
  except
    on E: Exception do
    begin
      WriteLn(E.Message, ', suppressed: ', Length(E.Suppressed));
      WriteLn(E.Suppressed[0].Message);
    end;
  end;
end.
