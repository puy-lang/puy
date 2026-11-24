program LazyValue;

// TLazy<T> calls its function once, at the first read of Value, for managed and plain types;
// Reset makes the next read call it again, and CreateValue starts already made
// (docs/spec/stdlib/Puy.Lazy/README.md 2).

uses
  System.SysUtils, Puy.Lazy;

type
  TPoint = record
    X, Y: Integer;
  end;

var
  Calls: Integer;
  S: TLazy<string>;
  A: TLazy<TArray<Integer>>;
  P: TLazy<TPoint>;
  N: TLazy<Integer>;
  Copy: TLazy<string>;

function MakeText: string;
begin
  Inc(Calls);
  Result := 'text ' + IntToStr(Calls);
end;

begin
  Calls := 0;
  S := TLazy<string>.Create(MakeText);
  WriteLn('made before reading: ', S.IsValueCreated, ', calls: ', Calls);
  WriteLn(S.Value);
  WriteLn(S.Value);
  WriteLn('made after reading: ', S.IsValueCreated, ', calls: ', Calls);

  // A copy of the reference shares the value
  Copy := S;
  WriteLn('copy: ', Copy.Value, ', calls: ', Calls);

  // Reset drops the value; the same function makes it again at the next read
  S.Reset;
  WriteLn('made after Reset: ', S.IsValueCreated);
  WriteLn(S.Value);
  S.Reset;
  S.Reset;
  WriteLn(S.Value, ', calls: ', Calls);

  A := TLazy<TArray<Integer>>.Create(
    function: TArray<Integer>
    begin
      WriteLn('making the array');
      Result := [1, 2, 3];
    end);
  WriteLn('array: ', Length(A.Value), ' ', A.Value[2]);

  P := TLazy<TPoint>.Create(
    function: TPoint
    begin
      WriteLn('making the point');
      Result := Default(TPoint);
      Result.X := 4;
      Result.Y := 5;
    end);
  WriteLn('point: ', P.Value.X, ' ', P.Value.Y);

  // Made from a value: no function is called
  N := TLazy<Integer>.CreateValue(42);
  WriteLn('from a value: ', N.IsValueCreated, ' ', N.Value);
end.
