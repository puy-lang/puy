unit Boxes;

interface

type
  // The same name as in `Pairs` with another number of type parameters: another name
  TPair<T> = record
    Only: T;
  end;

  // The same name and number of type parameters as in `Pairs`
  TBox<T> = record
    Item: T;
  end;

implementation

end.
