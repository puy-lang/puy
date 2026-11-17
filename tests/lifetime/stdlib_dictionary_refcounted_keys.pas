program StdlibDictionaryRefcountedKeys;

// A dictionary counts its keys and values (collections.md 3.1, 7.2): an instance of a reference
// counted class is not destroyed while it is a key or a value, and is let go of when its entry is
// removed, written over, cleared, or when the dictionary is freed. No leak.

uses System.SysUtils, System.Generics.Collections;

type
  TName = class(TRefObject)
  public
    Text: string;
    constructor Create(const AText: string);
    destructor Destroy; override;
  end;

constructor TName.Create(const AText: string);
begin
  inherited Create;
  Text := AText;
end;

destructor TName.Destroy;
begin
  WriteLn('  destroyed ', Text);
  inherited;
end;

var
  D: TDictionary<TName, TName>;

procedure AddPair(const Key, Value: string);
begin
  D.Add(TName.Create(Key), TName.Create(Value));
end;

// Each step is a routine of its own, so that the temporaries it makes go at its end
procedure RemoveFirst;
var
  K: TName;
begin
  K := D.Keys.ToArray[0];
  WriteLn('removing ', K.Text);
  D.Remove(K);
  WriteLn('removed, key still held: ', K.Text);
end;

procedure WriteOverFirst;
var
  K: TName;
begin
  K := D.Keys.ToArray[0];
  WriteLn('writing over the value of ', K.Text);
  D[K] := TName.Create('v2b');
end;

procedure ExtractFirst;
begin
  WriteLn('extracting');
  WriteLn('  extracted ', D.ExtractPair(D.Keys.ToArray[0]).Value.Text);
end;

begin
  D := TDictionary<TName, TName>.Create;
  AddPair('k1', 'v1');
  AddPair('k2', 'v2');
  AddPair('k3', 'v3');
  AddPair('k4', 'v4');
  WriteLn('added, count ', D.Count);
  RemoveFirst;
  WriteOverFirst;
  ExtractFirst;
  WriteLn('clearing');
  D.Clear;
  AddPair('k5', 'v5');
  WriteLn('freeing');
  D.Free;
  WriteLn('done');
end.
