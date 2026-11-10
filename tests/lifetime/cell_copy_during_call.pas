program CellCopyDuringCall;

// Copying the container while a field of it is passed by reference (lifetime.md 3.7): the copy
// `Y` keeps the values at the time of the copy, and the write through the cell goes to `R`
// only, because writing through the cell makes `R`'s block unique first.

type
  TInner = record
    N: Integer;
    Name: string;
  end;
  TRec = record
    In1: TInner;
    Items: array[0..1] of TInner;
  end;

var
  R, Y: TRec;

procedure Change(var X: Integer; var S: string);
begin
  Y := R;
  X := 7;
  S := 'new';
end;

begin
  R := Default(TRec);
  R.In1.Name := 'old';
  R.Items[1].Name := 'item';
  Change(R.In1.N, R.Items[1].Name);
  WriteLn(R.In1.N, ' ', Y.In1.N, ' ', R.Items[1].Name, ' ', Y.Items[1].Name);
end.
