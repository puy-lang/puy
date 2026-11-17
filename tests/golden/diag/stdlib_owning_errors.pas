program StdlibOwningErrors;

// The owning containers of the standard library (collections.md 9.1, 12.4; generics.md 3.6): a
// reference counted class or an interface for T is E0760 with a suggestion to use the container
// without ownership (in a declaration, in a `type` section, and in an expression), and the
// procedures Pop and Dequeue of an owning stack or queue used as values are E0725 with a note
// pointing to Extract. Through the parent types, Pop and Dequeue are functions.

uses System.Generics.Collections;

type
  TFoo = class
  end;

  TRefFoo = class(TRefObject)
  end;

  IFoo = interface
  end;

  THolder = class
    Items: TObjectStack<TRefFoo>;
  end;

var
  L: TObjectList<TRefFoo>;
  Q: TObjectQueue<IFoo>;
  S: TObjectStack<TFoo>;
  OQ: TObjectQueue<TFoo>;
  F: TFoo;
  PS: TStack<TFoo>;

begin
  L := nil;
  Q := nil;
  WriteLn(TObjectList<TRefFoo>.Create.Count);
  S := TObjectStack<TFoo>.Create;
  OQ := TObjectQueue<TFoo>.Create;
  F := S.Pop;
  F := S.Pop();
  WriteLn(OQ.Dequeue.ClassName);
  PS := S;
  F := PS.Pop;
  F := S.Extract;
end.
