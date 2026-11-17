program Owning;

uses
  System.SysUtils, System.Generics.Collections;

// Containers that own manually managed objects. TObjectList<T> frees an element when it is
// deleted, removed, replaced or cleared, and frees the rest when the list itself is freed;
// Extract hands an element back without freeing it. TObjectStack<T>.Pop and
// TObjectQueue<T>.Dequeue free the element (use Extract to keep it). TObjectDictionary<K, V>
// frees the keys and / or values it owns. The type argument must be a class that is not
// reference counted (the NotRefCounted constraint), because reference counted objects free
// themselves.

type
  TDocument = class
  private
    FName: string;
  public
    constructor Create(const AName: string);
    destructor Destroy; override;
    property Name: string read FName;
  end;

constructor TDocument.Create(const AName: string);
begin
  inherited Create;
  FName := AName;
end;

destructor TDocument.Destroy;
begin
  WriteLn('    (', FName, ' freed)');
  inherited;
end;

procedure Lists;
var
  Docs: TObjectList<TDocument>;
  Kept: TDocument;
begin
  WriteLn('TObjectList<TDocument>:');
  Docs := TObjectList<TDocument>.Create;
  try
    Docs.Add(TDocument.Create('report'));
    Docs.Add(TDocument.Create('draft'));
    Docs.Add(TDocument.Create('notes'));
    Docs.Add(TDocument.Create('memo'));
    WriteLn('  Delete(1):');
    Docs.Delete(1);
    WriteLn('  Docs[0] := summary:');
    Docs[0] := TDocument.Create('summary');
    WriteLn('  Extract(notes) keeps it alive:');
    Kept := Docs.Extract(Docs[1]);
    WriteLn('    extracted ', Kept.Name, ', ', Docs.Count, ' left');
    WriteLn('  freeing the list:');
  finally
    Docs.Free;
  end;
  WriteLn('  ', Kept.Name, ' is still ours to free:');
  Kept.Free;

  // OwnsObjects = False: the list only refers to objects owned elsewhere.
  WriteLn('TObjectList<TDocument>.Create(False):');
  Kept := TDocument.Create('shared');
  Docs := TObjectList<TDocument>.Create(False);
  try
    Docs.Add(Kept);
    Docs.Clear;
    WriteLn('  cleared without freeing ', Kept.Name);
  finally
    Docs.Free;
  end;
  Kept.Free;
end;

procedure StacksAndQueues;
var
  History: TObjectStack<TDocument>;
  Inbox: TObjectQueue<TDocument>;
  Doc: TDocument;
begin
  WriteLn('TObjectStack<TDocument> and TObjectQueue<TDocument>:');
  History := TObjectStack<TDocument>.Create;
  Inbox := TObjectQueue<TDocument>.Create;
  try
    History.Push(TDocument.Create('v1'));
    History.Push(TDocument.Create('v2'));
    WriteLn('  Pop frees the top:');
    History.Pop;
    Inbox.Enqueue(TDocument.Create('letter'));
    Inbox.Enqueue(TDocument.Create('invoice'));
    Doc := Inbox.Extract;
    WriteLn('  Extract hands back ', Doc.Name, '; Peek is ', Inbox.Peek.Name);
    Doc.Free;
    WriteLn('  freeing the stack and the queue:');
  finally
    History.Free;
    Inbox.Free;
  end;
end;

procedure Dictionaries;
var
  Files: TObjectDictionary<string, TDocument>;
begin
  WriteLn('TObjectDictionary<string, TDocument> owning its values:');
  Files := TObjectDictionary<string, TDocument>.Create([TDictionaryOwnership.doOwnsValues]);
  try
    Files.Add('a.txt', TDocument.Create('alpha'));
    Files.Add('b.txt', TDocument.Create('beta'));
    WriteLn('  replacing b.txt frees the old value:');
    Files['b.txt'] := TDocument.Create('beta 2');
    WriteLn('  Remove(a.txt):');
    Files.Remove('a.txt');
    WriteLn('  b.txt is ', Files['b.txt'].Name);
    WriteLn('  freeing the dictionary:');
  finally
    Files.Free;
  end;
end;

begin
  Lists;
  StacksAndQueues;
  Dictionaries;
end.
