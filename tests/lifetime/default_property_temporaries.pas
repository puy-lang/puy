// puy-test: memory-limit=1m
program DefaultPropertyTemporaries;

// The values a default array property returns are temporaries like a function's result
// (objects.md 12.1, 12.3): instances of a reference counted class read through `X[I]` and
// `X[L..H]` and dropped, the `TIndexRange` each range makes, and the copies `Clamp` takes are let
// go, so the loop stays inside the 1 MB limit, every instance is destroyed, and nothing is left at
// the end.

type
  TChunk = class(TRefObject)
    Data: array of Integer;
    constructor Create(N: Integer);
    destructor Destroy; override;
  end;

  TStore = class(TRefObject)
    function GetChunk(I: Integer): TChunk;
    function GetRange(const R: TIndexRange): TChunk;
    property Chunks[I: Integer]: TChunk read GetChunk; default;
    property Chunks[const R: TIndexRange]: TChunk read GetRange; default;
  end;

var
  Destroyed: Integer;

constructor TChunk.Create(N: Integer);
begin
  SetLength(Data, N);
end;

destructor TChunk.Destroy;
begin
  Inc(Destroyed);
  inherited;
end;

function TStore.GetChunk(I: Integer): TChunk;
begin
  Result := TChunk.Create(1000 + I);
end;

function TStore.GetRange(const R: TIndexRange): TChunk;
var
  F, L: Integer;
begin
  R.Clamp(0, 999, F, L);
  Result := TChunk.Create(L - F + 1);
end;

procedure Run;
var
  S: TStore;
  I, Total: Integer;
begin
  S := TStore.Create;
  Total := 0;
  for I := 1 to 2000 do
  begin
    Total := Total + Length(S[I mod 3].Data);
    Total := Total + Length(S[10..I mod 500].Data);
    Total := Total + Length(S[..].Data);
  end;
  WriteLn('total ', Total);
end;

begin
  Run;
  WriteLn('destroyed ', Destroyed);
end.
