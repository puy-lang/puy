// puy-test: test-ext
// Records and dynamic arrays passed to and returned from the functions of an extension
// (`docs/spec/extensions.md` 3): copied field by field and element by element, nested, flat in
// their containers or kept as blocks, at their default values, and with a callback.
program ExtRecords;

uses TestExt;

procedure ShowSpans(const A: TArray<TSpan>);
var
  S: TSpan;
begin
  for S in A do
    Write(S.Start, '+', S.Length, ' ');
  WriteLn('(', Length(A), ')');
end;

var
  S, T: TSpan;
  Empty: TSpan;
  A: TArray<Integer>;
  Names: TArray<string>;
  M: TArray<TArray<Double>>;
  Parts: TArray<TArray<Byte>>;
  B: TArray<Byte>;
  E: TEntry;
  Es: TArray<TEntry>;
  U: TArray<UTF8String>;
  C: TColor;
  I: Integer;
begin
  S.Start := High(Int64) - 5;
  S.Length := 7;
  T := ShiftSpan(S, 5);
  WriteLn(T.Start, ' ', T.Length, ' ', S.Start);
  T := ShiftSpan(Empty, -1);
  WriteLn(T.Start, ' ', T.Length);
  ShowSpans(RunsOf('aabxaaa日本aa', 'a'));
  ShowSpans(RunsOf('', 'a'));
  WriteLn(SumInts([1, 2, 3, 1000000000, 1000000000]), ' ', SumInts(nil));
  A := CountUp(5);
  for I in A do
    Write(I, ' ');
  WriteLn(Length(CountUp(0)));
  Names := UpperAll(['abc', 'été', '']);
  for I := 0 to High(Names) do
    Write('[', Names[I], '] ');
  WriteLn;
  M := Transpose([[1.5, 2.5, 3.5], [4.5, 5.5, 6.5]]);
  for I := 0 to High(M) do
    WriteLn(M[I][0]:0:1, ' ', M[I][1]:0:1);
  B := [1, 2, 0, 3, 0, 0, 4];
  Parts := SplitBytes(B);
  for I := 0 to High(Parts) do
    Write(Length(Parts[I]), ' ');
  WriteLn;
  for C in Rainbow do
    Write(Ord(C), ' ');
  WriteLn;
  WriteLn(Describe(E));
  U := ['zero', 'один', 'two'];
  Es := MakeEntries(U);
  for I := 0 to High(Es) do
    WriteLn(Describe(Es[I]));
  WriteLn(Es[2].Name, ' ', Es[2].Spans[1].Start, ' ', Es[1].Data[0], ' ', Es[1].Initial, ' ',
    Ord(Es[0].Color), ' ', Es[2].First.Start, ' ', Es[0].N7);
  E := Es[1];
  E.Name := 'changed';
  E.Spans := E.Spans + E.Spans;
  E.Spans[0] := ShiftSpan(E.Spans[0], 3);
  E.First := ShiftSpan(E.First, -1);
  WriteLn(Describe(E));
  WriteLn(Es[1].Name);
  A := MapInts(function(X: Integer): Integer begin Result := X * X end, [2, 3, 4]);
  for I in A do
    Write(I, ' ');
  WriteLn;
  WriteLn(SumLater([1, 2, 3]));
end.
