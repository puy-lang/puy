program LazyRelease;

// When TLazy<T> and TLazyObject<T> let go of what they hold (docs/spec/stdlib/Puy.Lazy/README.md
// 2.1): TLazy<T> releases a reference-counted value at Reset and at its own destruction but does
// not free a manually managed object; TLazyObject<T> frees the object it made at Reset and at its
// destruction, and nothing when the function raised; the function and what it captured live
// until the TLazy is destroyed (Reset makes the value again with it). A destructor that raises in
// Reset leaves the TLazyObject not made, so the next read makes a new object and nothing is freed
// twice. The function may drop the last reference to its TLazy: the caller holds the instance
// until the read returns (objects.md 4.1). Nothing leaks.

uses
  System.SysUtils, Puy.Lazy;

type
  TShared = class(TRefObject)
    FName: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
  end;

  TOwned = class
    FName: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
  end;

constructor TShared.Create(const AName: string);
begin
  inherited Create;
  FName := AName;
  WriteLn('create ', FName);
end;

destructor TShared.Destroy;
begin
  WriteLn('destroy ', FName);
  inherited Destroy;
end;

constructor TOwned.Create(const AName: string);
begin
  inherited Create;
  FName := AName;
  WriteLn('create ', FName);
end;

destructor TOwned.Destroy;
begin
  WriteLn('destroy ', FName);
  inherited Destroy;
end;

// Reads the value in a routine of its own, so that the temporary of the result is released
// before the caller goes on (objects.md 4.1).
function NameOf(const L: TLazy<TShared>): string;
begin
  Result := L.Value.FName;
end;

type
  TFailing = class
    destructor Destroy; override;
  end;

  TLoggedLazy = class(TLazy<string>)
    destructor Destroy; override;
  end;

destructor TFailing.Destroy;
begin
  WriteLn('destroy failing');
  inherited Destroy;
  raise EConvertError.Create('the destructor failed');
end;

destructor TLoggedLazy.Destroy;
begin
  WriteLn('destroy the TLazy');
  inherited Destroy;
end;

procedure SharedValue;
var
  L: TLazy<TShared>;
  Count: Integer;
begin
  Count := 0;
  L := TLazy<TShared>.Create(
    function: TShared
    begin
      Inc(Count);
      Result := TShared.Create('shared ' + IntToStr(Count));
    end);
  WriteLn('read ', NameOf(L));
  WriteLn('reset');
  L.Reset;
  WriteLn('read ', NameOf(L));
  WriteLn('drop the TLazy');
  L := nil;
  WriteLn('dropped');
end;

function MakeCapturing: TLazy<string>;
var
  Captured: TShared;
begin
  Captured := TShared.Create('captured');
  Result := TLazy<string>.Create(
    function: string
    begin
      Result := 'from ' + Captured.FName;
    end);
end;

procedure CapturedObject;
var
  L: TLazy<string>;
begin
  L := MakeCapturing;
  WriteLn('read ', L.Value);
  WriteLn('the function is kept after making the value');
  L.Reset;
  WriteLn('read ', L.Value);
  WriteLn('drop the TLazy');
  L := nil;
  WriteLn('dropped');
end;

procedure NotOwned;
var
  L: TLazy<TOwned>;
  O: TOwned;
begin
  L := TLazy<TOwned>.Create(
    function: TOwned
    begin
      Result := TOwned.Create('not owned');
    end);
  O := L.Value;
  WriteLn('drop the TLazy');
  L := nil;
  WriteLn('dropped; the object is still ', O.FName);
  O.Free;
end;

procedure Owned;
var
  L: TLazyObject<TOwned>;
  Count: Integer;
begin
  Count := 0;
  L := TLazyObject<TOwned>.Create(
    function: TOwned
    begin
      Inc(Count);
      if Count = 1 then
        raise EConvertError.Create('first attempt failed');
      Result := TOwned.Create('owned ' + IntToStr(Count));
    end);
  try
    WriteLn(L.Value.FName);
  except
    on E: EConvertError do
      WriteLn('raised: ', E.Message);
  end;
  WriteLn('read ', L.Value.FName);
  WriteLn('reset');
  L.Reset;
  WriteLn('made after Reset: ', L.IsValueCreated);
  WriteLn('read ', L.Value.FName);
  WriteLn('drop the TLazyObject');
  L := nil;
  WriteLn('dropped');

  // Never read: nothing to free
  L := TLazyObject<TOwned>.Create(
    function: TOwned
    begin
      Result := TOwned.Create('never made');
    end);
  L := nil;

  // Made by CreateValue: owned as well
  L := TLazyObject<TOwned>.CreateValue(TOwned.Create('given'));
  WriteLn('drop the TLazyObject');
  L := nil;
  WriteLn('dropped');
end;

procedure DestructorRaises;
var
  L: TLazyObject<TFailing>;
  Count: Integer;
begin
  Count := 0;
  L := TLazyObject<TFailing>.Create(
    function: TFailing
    begin
      Inc(Count);
      WriteLn('make ', Count);
      Result := TFailing.Create;
    end);
  WriteLn('made: ', L.Value <> nil);
  try
    L.Reset;
  except
    on E: EConvertError do
      WriteLn('Reset raised: ', E.Message);
  end;
  WriteLn('made after Reset: ', L.IsValueCreated);
  WriteLn('made again: ', L.Value <> nil);
  try
    L.Reset;
  except
    on E: EConvertError do
      WriteLn('Reset raised again: ', E.Message);
  end;
end;

var
  Global: TLazy<string>;

// Creates Global in a routine of its own, so that the temporary of the constructor is released
// before the read.
procedure MakeGlobal;
begin
  Global := TLoggedLazy.Create(
    function: string
    begin
      Global := nil;
      WriteLn('dropped the last reference while making');
      Result := 'made';
    end);
end;

// Reads Global in a routine of its own, so that the caller's hold on the instance ends when
// the routine does.
procedure ReadDroppingGlobal;
begin
  WriteLn('read ', Global.Value);
  WriteLn('the global is nil: ', Global = nil);
end;

begin
  SharedValue;
  WriteLn('--');
  CapturedObject;
  WriteLn('--');
  NotOwned;
  WriteLn('--');
  Owned;
  WriteLn('--');
  DestructorRaises;
  WriteLn('--');
  MakeGlobal;
  ReadDroppingGlobal;
  WriteLn('after the read');
end.
