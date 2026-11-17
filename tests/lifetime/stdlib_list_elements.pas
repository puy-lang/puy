program StdlibListElements;

// TList<T> counts its items like any other holder (collections.md 3.1): a reference-counted
// object stays alive while the list holds it and goes when the last item is let go of, by
// Delete, Clear, overwriting, a smaller Count, Extract (which hands the reference to the caller)
// or Free of the list. Manual objects are not owned. Freeing the list in the middle of a for..in
// makes the next MoveNext raise EAccessViolation (3.4). No leak.

uses System.SysUtils, System.Generics.Collections;

type
  TItem = class(TRefObject)
    Name: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
  end;

  TPlain = class
    Name: string;
    destructor Destroy; override;
  end;

constructor TItem.Create(const AName: string);
begin
  inherited Create;
  Name := AName;
end;

destructor TItem.Destroy;
begin
  WriteLn('  destroy ', Name);
  inherited;
end;

destructor TPlain.Destroy;
begin
  WriteLn('  free plain ', Name);
  inherited;
end;

// The temporaries of a call live until the end of the routine (lifetime.md), so the items are
// made in routines of their own to see when the list lets them go.
procedure AddNew(L: TList<TItem>; const Name: string);
begin
  L.Add(TItem.Create(Name));
end;

procedure InsertNew(L: TList<TItem>; Index: Integer; const Name: string);
begin
  L.Insert(Index, TItem.Create(Name));
end;

procedure SetNew(L: TList<TItem>; Index: Integer; const Name: string);
begin
  L[Index] := TItem.Create(Name);
end;

procedure Items;
var
  L: TList<TItem>;
  Kept: TItem;
begin
  L := TList<TItem>.Create;
  AddNew(L, 'a');
  AddNew(L, 'b');
  AddNew(L, 'c');
  AddNew(L, 'd');
  AddNew(L, 'e');
  InsertNew(L, 0, 'f');
  WriteLn('Delete(0)');
  L.Delete(0);
  WriteLn('L[0] := g');
  SetNew(L, 0, 'g');
  WriteLn('L[1] := L[1]');
  L[1] := L[1];
  WriteLn('Extract');
  Kept := L.ExtractAt(1);
  WriteLn('  kept ', Kept.Name);
  WriteLn('Count := 2');
  L.Count := 2;
  WriteLn('Exchange, Move, Reverse');
  L.Exchange(0, 1);
  L.Move(0, 1);
  L.Reverse;
  WriteLn('Clear');
  L.Clear;
  AddNew(L, 'h');
  L.Add(Kept);
  WriteLn('Kept := nil');
  Kept := nil;
  WriteLn('Free');
  L.Free;
  WriteLn('end of Items');
end;

procedure Strings;
var
  L: TList<string>;
  S: string;
begin
  L := TList<string>.Create;
  S := 'x';
  S := S + 'y';
  L.Add(S);
  L.Add(S + 'z');
  L.Delete(0);
  WriteLn('string ', L[0], ' ', S);
  L.Free;
end;

procedure Plain;
var
  L: TList<TPlain>;
  P: TPlain;
begin
  P := TPlain.Create;
  P.Name := 'p';
  L := TList<TPlain>.Create;
  L.Add(P);
  L.Free;
  WriteLn('list freed, plain still there: ', P.Name);
  P.Free;
end;

procedure FreedDuringEnumeration;
var
  L: TList<TItem>;
  X: TItem;
begin
  L := TList<TItem>.Create;
  AddNew(L, 'i');
  AddNew(L, 'j');
  try
    for X in L do
    begin
      WriteLn('  visit ', X.Name);
      L.Free;
      WriteLn('  freed');
    end;
  except
    on E: Exception do
      WriteLn('  ', E.ClassName);
  end;
  WriteLn('end of FreedDuringEnumeration');
end;

begin
  Items;
  Strings;
  Plain;
  WriteLn('FreedDuringEnumeration');
  FreedDuringEnumeration;
end.
