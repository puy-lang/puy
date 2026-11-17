program DefaultPropertyMoreErrors;

// More errors of default array properties (objects.md 12.2, 12.3): `TIndexRange` values compare
// as records without `=` (E0703), and a child interface's default property hides all of its
// parent's, so an index only the parent's takes is E0702.

type
  IA = interface
    function GetI(I: Integer): Integer;
    property Items[I: Integer]: Integer read GetI; default;
  end;

  IB = interface(IA)
    function GetS(const S: string): Integer;
    property Items[const S: string]: Integer read GetS; default;
  end;

procedure Compare(const R, S: TIndexRange);
begin
  if R = S then                                    // E0703
    WriteLn('same');
end;

var
  B: IB;
begin
  B := nil;
  WriteLn(B[1]);                                   // E0702: `IA`'s is hidden
  WriteLn(B['a'], B.GetI(1));
end.
