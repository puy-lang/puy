program ConstCopyOnWrite;

// A typed constant's value is in the constant cache, which holds a reference of its own
// (types.md chapter 4). Writing to a variable that got the constant copies the block first
// (`Unique`), so the constant does not change, however often it is read.

type
  TPoint = record
    X, Y: Integer;
    Tag: string;
  end;

const
  Origin: TPoint = (X: 0; Y: 0; Tag: 'origin');

procedure Use;
var
  P: TPoint;
begin
  P := Origin;
  P.X := P.X + 1;
  P.Tag := 'moved';
  WriteLn(P.X, ' ', P.Tag, ' ', Origin.X, ' ', Origin.Tag);
end;

begin
  Use;
  Use;
end.
