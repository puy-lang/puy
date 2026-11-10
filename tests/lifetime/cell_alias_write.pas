program CellAliasWrite;

// A field passed to a `var` parameter is a place reference (lifetime.md 3.7): the cell holds the
// global `R` and the path to `F`, not `R`'s block. When the callee first writes another field of
// `R` (which makes `R` unique), the write through the cell still lands in the `R` seen after
// the call, as in Delphi.

type
  TRec = record
    F, G: Integer;
    S: string;
  end;

var
  R: TRec;

procedure Touch(var X: Integer; var T: string);
begin
  R.G := 1;
  R.S := 'g';
  X := 5;
  T := T + '!';
end;

begin
  R := Default(TRec);
  R.S := 's';
  Touch(R.F, R.S);
  WriteLn(R.F, ' ', R.G, ' ', R.S);
end.
