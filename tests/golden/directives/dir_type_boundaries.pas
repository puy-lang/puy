program DirTypeBoundaries;

// Directives between type declarations and between record fields are at a boundary
// (directives.md 3.1); inside a field's type they are not (W0005).

type
  {$IFDEF PUY}
  TA = record
    {$R-}
    X: Integer;
    {$IFDEF NEVER}
    Y: string;
    {$ENDIF}
    Z: array[0..{$R+}1] of Integer;
  end;
  {$ENDIF}
  TB = array[0..1] of Integer;

var
  A: TA;
begin
  A := Default(TA);
  A.X := 2;
  WriteLn(A.X, ' ', Length(A.Z));
end.
