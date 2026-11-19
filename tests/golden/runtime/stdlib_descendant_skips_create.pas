program StdlibDescendantSkipsCreate;

// A descendant of TStack<T> or TQueue<T> whose constructor does not call the inherited one still
// works, and its Notify is called (collections.md 9.4). A descendant of TDictionary<K, V> or
// THashSet<T> must call an inherited Create (9.5): without it, the members that touch the items
// raise EAccessViolation.

uses System.SysUtils, System.Generics.Collections;

type
  TMyStack = class(TStack<Integer>)
  protected
    procedure Notify(const Item: Integer; Action: TCollectionNotification); override;
  public
    constructor Create;
  end;

  TMyQueue = class(TQueue<Integer>)
  protected
    procedure Notify(const Item: Integer; Action: TCollectionNotification); override;
  public
    constructor Create;
  end;

  TMyDictionary = class(TDictionary<string, Integer>)
  public
    constructor Create;
  end;

  TMySet = class(THashSet<Integer>)
  public
    constructor Create;
  end;

procedure Told(const Who: string; Item: Integer; Action: TCollectionNotification);
begin
  if Action = TCollectionNotification.cnAdding then
    WriteLn('  ', Who, ' adding ', Item)
  else
    WriteLn('  ', Who, ' removed ', Item);
end;

procedure TMyStack.Notify(const Item: Integer; Action: TCollectionNotification);
begin
  Told('stack', Item, Action);
end;

constructor TMyStack.Create;
begin
end;

procedure TMyQueue.Notify(const Item: Integer; Action: TCollectionNotification);
begin
  Told('queue', Item, Action);
end;

constructor TMyQueue.Create;
begin
end;

constructor TMyDictionary.Create;
begin
end;

constructor TMySet.Create;
begin
end;

procedure Report(const E: Exception);
begin
  WriteLn('  ', E.ClassName, ': ', E.Message);
end;

var
  S: TMyStack;
  Q: TMyQueue;
  D: TMyDictionary;
  H: TMySet;
begin
  WriteLn('stack');
  S := TMyStack.Create;
  S.Push(1);
  S.Push(2);
  WriteLn('  pop ', S.Pop, ', count ', S.Count);
  S.Free;

  WriteLn('queue');
  Q := TMyQueue.Create;
  Q.Enqueue(1);
  Q.Enqueue(2);
  WriteLn('  dequeue ', Q.Dequeue, ', count ', Q.Count);
  Q.Free;

  WriteLn('dictionary');
  D := TMyDictionary.Create;
  try D.Add('a', 1); except on E: Exception do Report(E); end;
  try D.ContainsKey('a'); except on E: Exception do Report(E); end;
  D.Free;

  WriteLn('set');
  H := TMySet.Create;
  try H.Add(1); except on E: Exception do Report(E); end;
  try H.Contains(1); except on E: Exception do Report(E); end;
  H.Free;
end.
