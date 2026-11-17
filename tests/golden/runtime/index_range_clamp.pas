program IndexRangeClamp;

// `X[L..H]` on a type with a default property taking `TIndexRange` (objects.md 12.3): the range
// is a new `TIndexRange` with the bounds written, `Clamp` cuts it to the bounds the type gives
// like the built-in slices (types.md 8.7; an empty result has `ALast = AFirst - 1`), a range may
// be any index of several, a range is written through a `write` accessor, a `TIndexRange` made
// by hand is passed like any other value, and the places of `AFirst` and `ALast` are fixed before
// either is written, as for a call (`Arr[I]` is the element of `I` before `Clamp`).

uses
  System.SysUtils;

type
  TBox = class(TRefObject)
    N: Integer;
  end;

  TSeq = class(TRefObject)
  private
    FData: array[0..9] of Integer;
    function GetItem(I: Integer): Integer;
    function GetRange(const R: TIndexRange): TArray<Integer>;
    procedure SetRange(const R: TIndexRange; const Value: TArray<Integer>);
    function GetRow(const R: TIndexRange; Col: Integer): string;
  public
    constructor Create;
    property Items[I: Integer]: Integer read GetItem; default;
    property Items[const R: TIndexRange]: TArray<Integer> read GetRange write SetRange; default;
    property Items[const R: TIndexRange; Col: Integer]: string read GetRow; default;
  end;

constructor TSeq.Create;
var
  I: Integer;
begin
  for I := 0 to 9 do
    FData[I] := I * 10;
end;

function TSeq.GetItem(I: Integer): Integer;
begin
  Result := FData[I];
end;

function TSeq.GetRange(const R: TIndexRange): TArray<Integer>;
var
  F, L, I: Integer;
begin
  R.Clamp(0, 9, F, L);
  SetLength(Result, L - F + 1);
  for I := F to L do
    Result[I - F] := FData[I];
end;

procedure TSeq.SetRange(const R: TIndexRange; const Value: TArray<Integer>);
var
  F, L, I: Integer;
begin
  R.Clamp(0, 9, F, L);
  for I := F to L do
    FData[I] := Value[I - F];
end;

function TSeq.GetRow(const R: TIndexRange; Col: Integer): string;
begin
  Result := IntToStr(Length(GetRange(R))) + ' at ' + IntToStr(Col);
end;

procedure Show(const A: TArray<Integer>);
var
  X: Integer;
begin
  Write(Length(A), ':');
  for X in A do
    Write(' ', X);
  WriteLn;
end;

procedure ShowClamp(const R: TIndexRange; Low, High: Integer);
var
  F, L: Integer;
begin
  R.Clamp(Low, High, F, L);
  WriteLn(R.First, ' ', R.Last, ' ', R.HasFirst, ' ', R.HasLast, ' -> ', F, ' ', L);
end;

var
  S: TSeq;
  R: TIndexRange;
  Lo, Hi, I: Integer;
  Arr: array[0..9] of Integer;
  Boxes: array[0..1] of TBox;
begin
  S := TSeq.Create;
  Show(S[2..4]);
  Show(S[7..]);
  Show(S[..1]);
  Show(S[..]);
  Show(S[-5..100]);
  Show(S[6..3]);
  Show(S[20..30]);
  Lo := 8;
  Hi := 9;
  Show(S[Lo..Hi]);
  WriteLn(S[1..3, 7]);
  S[0..1] := [5, 6];
  Show(S[..2]);
  WriteLn(S[4]);
  ShowClamp(R, 0, 9);
  R.First := 3;
  R.HasFirst := True;
  ShowClamp(R, 0, 9);
  R.Last := 20;
  R.HasLast := True;
  ShowClamp(R, 0, 9);
  ShowClamp(R, 5, 6);
  ShowClamp(R, 0, -1);
  Show(S[R]);
  I := 0;
  R.Clamp(0, 9, I, Arr[I]);
  WriteLn(I, ' ', Arr[0], ' ', Arr[3]);
  Boxes[0] := TBox.Create;
  Boxes[1] := TBox.Create;
  I := 0;
  R.Clamp(1, 9, I, Boxes[I].N);
  WriteLn(I, ' ', Boxes[0].N, ' ', Boxes[1].N);
end.
