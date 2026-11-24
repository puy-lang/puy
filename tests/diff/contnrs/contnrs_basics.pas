program ContnrsBasics;

// The containers of System.Contnrs (docs/spec/stdlib/System.Contnrs/README.md): TObjectList
// owning and not owning its objects, FindInstanceOf, TClassList, and the order of TObjectStack
// and TObjectQueue. A destructor writes a line, so the output shows which objects are freed.

uses System.SysUtils, System.Contnrs;

type
  TItem = class
  public
    Name: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
  end;

  TSpecial = class(TItem)
  end;

constructor TItem.Create(const AName: string);
begin
  inherited Create;
  Name := AName;
end;

destructor TItem.Destroy;
begin
  WriteLn('free ', Name);
  inherited Destroy;
end;

procedure Owning;
var
  L: TObjectList;
  A: TItem;
  I: Integer;
begin
  L := TObjectList.Create;
  WriteLn('owns=', L.OwnsObjects);
  L.Add(TItem.Create('a'));
  L.Add(TSpecial.Create('b'));
  L.Add(TItem.Create('c'));
  L.Insert(0, TSpecial.Create('d'));
  for I := 0 to L.Count - 1 do
    Write(TItem(L[I]).Name, ' ');
  WriteLn('count=', L.Count);
  WriteLn('first=', TItem(L.First).Name, ' last=', TItem(L.Last).Name);
  WriteLn('exact TItem from 0: ', L.FindInstanceOf(TItem, True, 0));
  WriteLn('exact TSpecial from 0: ', L.FindInstanceOf(TSpecial, True, 0));
  WriteLn('exact TSpecial from 1: ', L.FindInstanceOf(TSpecial, True, 1));
  WriteLn('derived TItem from 0: ', L.FindInstanceOf(TItem, False, 0));
  WriteLn('exact TSpecial from 3: ', L.FindInstanceOf(TSpecial, True, 3));
  L.Delete(1);
  A := TItem(L.Extract(L[0]));
  WriteLn('extracted ', A.Name, ' count=', L.Count);
  A.Free;
  WriteLn('index of c: ', L.IndexOf(L[1]));
  L.Remove(L[1]);
  WriteLn('count=', L.Count);
  L.Free;
  WriteLn('owning done');
end;

procedure NotOwning;
var
  L: TObjectList;
  A, B: TItem;
begin
  A := TItem.Create('x');
  B := TItem.Create('y');
  L := TObjectList.Create(False);
  WriteLn('owns=', L.OwnsObjects);
  L.Add(A);
  L.Add(B);
  L.Clear;
  WriteLn('count=', L.Count);
  L.Add(A);
  L.Free;
  WriteLn('list freed');
  A.Free;
  B.Free;
end;

procedure Classes;
var
  L: TClassList;
begin
  L := TClassList.Create;
  L.Add(TItem);
  L.Add(TSpecial);
  WriteLn(L.Count, ' ', L[0].ClassName, ' ', L[1].ClassName);
  WriteLn('index of TSpecial: ', L.IndexOf(TSpecial));
  WriteLn('index of TObject: ', L.IndexOf(TObject));
  L.Free;
end;

procedure StackAndQueue;
var
  S: TObjectStack;
  Q: TObjectQueue;
  A, B, C: TItem;
begin
  A := TItem.Create('1');
  B := TItem.Create('2');
  C := TItem.Create('3');
  S := TObjectStack.Create;
  Q := TObjectQueue.Create;
  WriteLn('pushed ', TItem(S.Push(A)).Name);
  S.Push(B);
  S.Push(C);
  Q.Push(A);
  Q.Push(B);
  Q.Push(C);
  WriteLn('stack count=', S.Count, ' at least 3: ', S.AtLeast(3), ' at least 4: ', S.AtLeast(4));
  WriteLn('stack peek ', TItem(S.Peek).Name);
  Write('stack');
  while S.Count > 0 do
    Write(' ', TItem(S.Pop).Name);
  WriteLn;
  WriteLn('queue peek ', TItem(Q.Peek).Name);
  Write('queue');
  while Q.Count > 0 do
    Write(' ', TItem(Q.Pop).Name);
  WriteLn;
  S.Push(A);
  S.Free;
  Q.Free;
  WriteLn('stack and queue freed');
  A.Free;
  B.Free;
  C.Free;
end;

begin
  Owning;
  NotOwning;
  Classes;
  StackAndQueue;
end.
