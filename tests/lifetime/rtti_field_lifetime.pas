// puy-test: memory-limit=1m
program RttiFieldLifetime;

// Reading and writing fields through System.Rtti counts what it reads and writes like the
// fields' own reads and writes (rtti.md 5.6): an instance of a reference counted class written
// into a field lives while the field holds it and goes when it is overwritten; strings, arrays
// and records read and written in a loop are let go of (the loop would run out of the 1 MB limit
// if anything were kept); the TRttiType and member objects live until the isolate ends and
// leave nothing behind.

uses System.SysUtils, System.Rtti;

type
  TNode = class(TRefObject)
  public
    Name: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
  end;

  TRec = record
    S: string;
    A: TArray<string>;
  end;

  THolder = class(TRefObject)
  public
    Node: TNode;
    Text: string;
    Rec: TRec;
    Items: TArray<Integer>;
  end;

constructor TNode.Create(const AName: string);
begin
  Name := AName;
end;

destructor TNode.Destroy;
begin
  WriteLn('destroy ', Name);
end;

// Writes two nodes in turn: the first goes when the second takes its place.
procedure Swap(H: THolder);
begin
  var F := TRttiContext.Create.GetType(THolder).GetField('Node');
  F.SetValue(H, TNode.Create('first'));
  WriteLn('wrote first');
  F.SetValue(H, TNode.Create('second'));
  WriteLn('wrote second');
end;

procedure Churn(H: THolder);
begin
  var T := TRttiContext.Create.GetType(THolder);
  var R := Default(TRec);
  for var I := 1 to 2000 do
  begin
    R.S := StringOfChar('x', 200);
    R.A := [R.S, R.S + 'y'];
    T.GetField('Text').SetValue(H, R.S + IntToStr(I));
    T.GetField('Rec').SetValue(H, TValue.From<TRec>(R));
    T.GetField('Items').SetValue(H, TValue.From<TArray<Integer>>([I, I + 1]));
    var V := T.GetField('Rec').GetValue(H);
    var Box := TValue.From<TRec>(R);
    T.GetField('Rec').GetValue(H);
    TRttiContext.Create.GetType(TypeInfo(TRec)).GetField('S').SetValue(Box,
      T.GetField('Text').GetValue(H));
    if V.AsType<TRec>.S <> R.S then
      WriteLn('wrong value');
  end;
  WriteLn(Length(H.Text), ' ', Length(H.Rec.A), ' ', H.Items[1]);
end;

procedure Run;
begin
  var H := THolder.Create;
  Swap(H);
  WriteLn('after Swap: ', H.Node.Name);
  Churn(H);
end;

begin
  Run;
  WriteLn('end');
end.
