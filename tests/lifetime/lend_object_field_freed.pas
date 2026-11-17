program LendObjectFieldFreed;

// A managed value lent from a field of an instance whose instance is freed during the call is
// let go when it cannot be given back (objects.md 2.2, 14.3): a string field, and a string in a
// record field (a path of two steps). The `EAccessViolation` is caught and nothing leaks. The
// other argument of the same call is given back to its live instance.

uses
  System.SysUtils;

type
  TPair = record
    S: string;
    N: Integer;
  end;

  TBox = class
  public
    Name: string;
    Pair: TPair;
  end;

var
  G: TBox;

procedure Rewrite(var S, T: string);
begin
  S := S + ' changed';
  T := T + ' kept';
  G.Free;
end;

var
  Doomed, Live: TBox;
begin
  Live := TBox.Create;
  Live.Name := 'live';
  Doomed := TBox.Create;
  Doomed.Name := 'doomed';
  G := Doomed;
  try
    Rewrite(Doomed.Name, Live.Name);
  except
    on E: EAccessViolation do
      WriteLn('field: ', E.Message);
  end;
  WriteLn(Live.Name);
  Doomed := TBox.Create;
  Doomed.Pair.S := 'inner';
  G := Doomed;
  try
    Rewrite(Doomed.Pair.S, Live.Name);
  except
    on E: EAccessViolation do
      WriteLn('record field: ', E.Message);
  end;
  WriteLn(Live.Name);
  Live.Free;
end.
